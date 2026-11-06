//! The grammar of a ```questions block: the form an agent writes in its reply, which the chat draws
//! as a form (packages/gx-chat-core/src/transcript/question_forms.rs finds the blocks, and
//! `ghostex visual check` reads one the same way).
//!
//! ````text
//! ```questions
//! {
//!   "title": "Open questions",
//!   "questions": [
//!     {"tag": "Q1", "title": "Which layout?", "detail": "Why it matters.",
//!      "choices": [{"label": "Tabs", "detail": "One page per area.", "recommended": true},
//!                  "One long page"]},
//!     {"title": "Which platforms first?", "multiple": true, "choices": ["macOS", "Windows"]},
//!     {"title": "What should it be called?", "type": "text"}
//!   ]
//! }
//! ```
//! ````
//!
//! The model is the page kit's (`server/src/visual_pages/kit.rs`): a choice question ends with
//! "Other / none of these" and a note box, nothing is preselected, and the answers go back to the
//! agent as the same `<ghostex-page-answers>` message a page sends, keyed by each question's `name`
//! (its title when it has none), a note as `"<key> (note)"`, and a form of two or more questions
//! ends with a notes box whose text arrives last, as `"Additional notes"` ([`notes_key`]). A question
//! left blank when the form is sent, text questions too, arrives as the value `"skipped"` ([`SKIPPED`]).
//!
//! CDXC:SessionChat 2026-10-11 DECISION:
//! User: questions an agent asks are answered in a form inside the chat itself (desktop and phone), not one question at a time and not by typing letters: all questions at once, selectable choices with nothing preselected (clicking again clears), the automatic "Other / none of these" with a note box, text questions, and one Send button. Any agent writes it as a ```questions block, the answers go back as a message, and the form then shows as answered. Pages (gx-questions) stay only for questions that need visuals.
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! packages/gx-chat-core/src/transcript/native_markdown.rs marks the fence with this id; packages/gx-chat-core/src/questions/forms.rs keeps the reader's picks and sends them; apps/desktop/src/app/native_chat/question_form.rs and apps/mobile/app/src/chat/native/transcript/QuestionForm.tsx draw it; server/src/ghostex_cli/visual.rs checks a block; skills/ghostex-visuals/SKILL.md teaches the grammar. Change them together.

use serde::Serialize;
use serde_json::{Map, Value};

/// The choice every choice question ends with.
pub const OTHER_CHOICE: &str = "Other / none of these";
/// The answer a skipped question sends, whatever kind of question it is.
///
/// CDXC:SessionChat 2026-10-16 DECISION:
/// User: "i want a way to skip answering a question (maybe we send the agent something simple like "user skipped q155" ?? not sure u tell me about this one what's best options", then picked "Per-question Skip + Skip all", and later (2026-10-16): "i think we don't need a separate Skip option for each one of the question. if i send and that one is not answered consider it skipped". There is no Skip row on a question: Send answers sends the form with any question left blank marked skipped, and a "Skip all" button beside it sends at once with every question marked skipped and no notes, whatever was picked or typed (2026-10-20, user: "i picked an option for Q355 and then clicked on skip all but it went ahead and sent what i had picked.. this is wrong pls fix", superseding the earlier rule that answered questions keep their answers under Skip all). This supersedes the per-question Skip row the first choice had. The agent reads `"skipped"` under the question's key and moves on without asking again unless it blocks the work; a skipped question counts as answered, and its tag turns grey. Both buttons carry no keycap: Ctrl+Enter (Send answers) and Ctrl+Shift+Enter (Skip all) show in a quick tooltip, and Skip all wears Send's colours a shade darker (2026-10-16).
pub const SKIPPED: &str = "skipped";
/// How a question's note is keyed in the answers.
pub const NOTE_SUFFIX: &str = " (note)";
/// How the form's own notes box (a form of two or more questions) is keyed in the answers, after
/// every question's answer.
pub const NOTES_KEY: &str = "Additional notes";

const MAX_QUESTIONS: usize = 20;
const MAX_CHOICES: usize = 26;

/// Whether a fence's language names a questions block.
pub fn is_questions_language(language: &str) -> bool {
    language.trim().eq_ignore_ascii_case("questions")
}

/// One form, as the renderers draw it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionForm {
    pub id: String,
    pub title: String,
    /// The send button's label.
    pub submit: String,
    pub questions: Vec<FormQuestion>,
    /// The tags of earlier questions this block withdraws (`"withdraw": ["Q40"]`): they no longer
    /// count as unanswered. A block may hold only this, with no questions of its own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub withdraw: Vec<String>,
    /// Why the block could not be read, shown in place of the form.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    /// A block that could not be read and that Ghostex has since asked the agent to send again
    /// ([`resend_message`]); the chat sets it from the transcript.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub resent: bool,
    /// A one-click question written as answer links in a reply's prose (`"inline": true`, made by
    /// gx-chat-core's `transcript/inline_answers.rs`), not a ```questions block.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub inline: bool,
}

impl QuestionForm {
    /// A block that only withdraws earlier questions and asks none: the chat draws a line naming
    /// the withdrawn tags instead of a form.
    pub fn is_withdrawal(&self) -> bool {
        self.error.is_empty() && self.questions.is_empty() && !self.withdraw.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormQuestion {
    /// The answer's key: the question's `name`, else its title.
    pub key: String,
    pub tag: String,
    pub title: String,
    pub detail: String,
    pub multiple: bool,
    /// A text box instead of choices.
    pub text: bool,
    pub placeholder: String,
    pub choices: Vec<FormChoice>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormChoice {
    pub label: String,
    pub detail: String,
    /// What the answer carries: `value`, else the label.
    pub value: String,
    pub recommended: bool,
}

/// FNV-1a over `text`, from `basis`.
fn fnv(text: &str, basis: u64) -> u64 {
    text.bytes().fold(basis, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The form's id: 32 hex characters from its JSON with the spacing taken out, so every client (and
/// the answers message) names the same form the same way, however the reply was wrapped.
pub fn form_id(body: &str) -> String {
    let canonical = serde_json::from_str::<Value>(body)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| body.trim().to_string());
    format!(
        "{:016x}{:016x}",
        fnv(&canonical, 0xcbf2_9ce4_8422_2325),
        fnv(&canonical, 0x6c62_272e_07bb_0142)
    )
}

fn string(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

fn choice(value: &Value, number: usize, question: usize) -> Result<FormChoice, String> {
    let (label, object) = match value {
        Value::String(label) => (label.trim().to_string(), None),
        Value::Object(object) => (
            string(
                object
                    .get("label")
                    .or_else(|| object.get("title"))
                    .or_else(|| object.get("text")),
            ),
            Some(object),
        ),
        _ => {
            return Err(format!(
                "Choice {number} of question {question} must be text or an object with a \"label\"."
            ))
        }
    };
    if label.is_empty() {
        return Err(format!(
            "Choice {number} of question {question} needs a \"label\"."
        ));
    }
    let value = object
        .map(|object| string(object.get("value")))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| label.clone());
    Ok(FormChoice {
        detail: object
            .map(|object| string(object.get("detail").or_else(|| object.get("description"))))
            .unwrap_or_default(),
        recommended: object
            .and_then(|object| object.get("recommended"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        label,
        value,
    })
}

/// A tag written at the start of a question's title instead of in `"tag"` (`"Q198: Which key…"`),
/// and the title after it: `[A-Z]{1,8}-?[0-9]{1,6}` followed by a colon.
///
/// CDXC:SessionChat 2026-10-17 WHY:
/// Agents often write `"question": "Q198: …"` with no `"tag"`. Read as untagged, such a form could never be replaced by a later form asking Q198 again nor withdrawn, so it came back as unanswered and the Previous question pill sent the user to answer it a second time. The answer key stays the full title, so answers already sent still match.
fn title_tag(title: &str) -> Option<(&str, &str)> {
    let bytes = title.as_bytes();
    let letters = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_uppercase())
        .count();
    if letters == 0 || letters > 8 {
        return None;
    }
    let mut end = letters + usize::from(bytes.get(letters) == Some(&b'-'));
    let digits = bytes[end..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 || digits > 6 {
        return None;
    }
    end += digits;
    let rest = title[end..].strip_prefix(':')?.trim_start();
    (!rest.is_empty()).then(|| (&title[..end], rest))
}

/// A tag written as the question's `"id"` instead of its `"tag"` (`"id": "Q106"`), when the id is
/// shaped like one: `[A-Za-z]{1,8}-?[0-9]{1,6}` and nothing else.
///
/// CDXC:SessionChat 2026-10-22 WHY:
/// Agents often write `{"id": "Q106", "question": …, "options": …}`. Read as untagged, a later form asking Q106 again or `"withdraw": ["Q106"]` could never clear it, so the sidebar's pink question dot stayed on for questions the agent had already replaced or withdrawn. An id that is not tag-shaped (`"mDrive"`) is left alone.
fn id_tag(object: &Map<String, Value>) -> Option<String> {
    let id = string(object.get("id"));
    let bytes = id.as_bytes();
    let letters = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    let dash = usize::from(bytes.get(letters) == Some(&b'-'));
    let digits = bytes[letters + dash..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let shaped = (1..=8).contains(&letters)
        && (1..=6).contains(&digits)
        && letters + dash + digits == bytes.len();
    shaped.then_some(id)
}

fn question(
    object: &Map<String, Value>,
    number: usize,
    used: &mut Vec<String>,
) -> Result<FormQuestion, String> {
    let title = string(
        object
            .get("title")
            .or_else(|| object.get("question"))
            .or_else(|| object.get("text")),
    );
    if title.is_empty() {
        return Err(format!("Question {number} needs a \"title\"."));
    }
    let text = string(object.get("type")).eq_ignore_ascii_case("text");
    let choices = match object.get("choices").or_else(|| object.get("options")) {
        Some(Value::Array(choices)) if !text => choices
            .iter()
            .enumerate()
            .map(|(index, value)| choice(value, index + 1, number))
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };
    if !text && choices.is_empty() {
        return Err(format!(
            "Question {number} needs \"choices\", or \"type\": \"text\" for a text answer."
        ));
    }
    if choices.len() > MAX_CHOICES {
        return Err(format!(
            "Question {number} has {} choices; a question takes at most {MAX_CHOICES}.",
            choices.len()
        ));
    }
    let base = Some(string(object.get("name")))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| title.clone());
    let mut tag = string(object.get("tag"));
    let mut title = title;
    if tag.is_empty() {
        if let Some((written, rest)) = title_tag(&title) {
            (tag, title) = (written.to_string(), rest.to_string());
        }
    }
    if tag.is_empty() {
        tag = id_tag(object).unwrap_or_default();
    }
    let mut key = base.clone();
    let mut copy = 2;
    while used.contains(&key) {
        key = format!("{base} ({copy})");
        copy += 1;
    }
    used.push(key.clone());
    Ok(FormQuestion {
        key,
        tag,
        detail: string(object.get("detail").or_else(|| object.get("description"))),
        multiple: object
            .get("multiple")
            .or_else(|| object.get("multiSelect"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        placeholder: string(object.get("placeholder")),
        text,
        choices,
        title,
    })
}

/// A block's `withdraw` list: the tags of earlier questions it withdraws.
///
/// CDXC:SessionChat 2026-10-14 DECISION:
/// User (Q82): questions the agent marks obsolete drop off the chat's unanswered count. A later form or page that asks the same tag supersedes the earlier question, and `"withdraw": ["Q40"]` in a later ```questions block (alone, or beside new questions) withdraws a tag without asking anything.
fn withdrawn_tags(value: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or("\"withdraw\" must be a list of question tags, such as [\"Q40\"].")?;
    let mut tags: Vec<String> = Vec::new();
    for item in items {
        let tag = item
            .as_str()
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .ok_or("Each entry in \"withdraw\" must be a question tag, such as \"Q40\".")?;
        if !tags.iter().any(|known| known.eq_ignore_ascii_case(tag)) {
            tags.push(tag.to_string());
        }
    }
    Ok(tags)
}

/// The smart double quotes a word processor or a model writes for `"`.
fn is_smart_quote(character: char) -> bool {
    matches!(character, '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}')
}

/// `body` with the two slips of hand-written JSON mended: smart quotes around keys and strings
/// (an ASCII quote inside such a string is escaped; a smart quote inside an ordinary string stays
/// text) and a comma before a closing bracket. `None` when there is nothing to mend.
fn mend_json(body: &str) -> Option<String> {
    let mut mended = String::with_capacity(body.len());
    // Inside a string: `Some(true)` for one opened with `"`, `Some(false)` for a smart-quoted one.
    let mut string: Option<bool> = None;
    let mut escaped = false;
    let mut changed = false;
    for (at, character) in body.char_indices() {
        match string {
            Some(_) if escaped => {
                escaped = false;
                mended.push(character);
            }
            Some(_) if character == '\\' => {
                escaped = true;
                mended.push(character);
            }
            Some(true) if character == '"' => {
                string = None;
                mended.push(character);
            }
            Some(false) if character == '"' => {
                mended.push_str("\\\"");
                changed = true;
            }
            Some(false) if is_smart_quote(character) => {
                string = None;
                mended.push('"');
            }
            Some(_) => mended.push(character),
            None if character == '"' => {
                string = Some(true);
                mended.push(character);
            }
            None if is_smart_quote(character) => {
                string = Some(false);
                mended.push('"');
                changed = true;
            }
            None if character == ',' && body[at + 1..].trim_start().starts_with([']', '}']) => {
                changed = true;
            }
            None => mended.push(character),
        }
    }
    changed.then_some(mended)
}

/// Whether an object is one question written on its own, not a block: it has a question's fields.
fn is_lone_question(object: &Map<String, Value>) -> bool {
    ["question", "choices", "options"]
        .iter()
        .any(|key| object.contains_key(*key))
        || string(object.get("type")).eq_ignore_ascii_case("text")
}

/// Whether JSON given with no fence (`ghostex visual check <file.json>`) is meant as a
/// ```questions block rather than a ```visual one: a list, an object with `questions` or
/// `withdraw`, or one question on its own, read as [`parse_form`] reads it. JSON that cannot be
/// read at all counts when it mentions "questions", so it gets the form's error.
pub fn looks_like_questions(source: &str) -> bool {
    let value = serde_json::from_str::<Value>(source).ok().or_else(|| {
        mend_json(source).and_then(|mended| serde_json::from_str::<Value>(&mended).ok())
    });
    match value {
        Some(Value::Array(_)) => true,
        Some(Value::Object(object)) => {
            object.contains_key("questions")
                || object.contains_key("withdraw")
                || is_lone_question(&object)
        }
        Some(_) => false,
        None => source.contains("\"questions\""),
    }
}

/// A tag's prefix and number: `("Q", 12)` for `Q12`, `("Q-", 12)` for `Q-12`.
fn tag_number(tag: &str) -> Option<(&str, u64)> {
    let digits = tag.len() - tag.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (prefix, number) = tag.split_at(tag.len() - digits);
    Some((prefix, number.parse().ok()?))
}

/// Numbers the questions written with no tag from their tagged neighbours (Q4, ?, Q6 makes the
/// middle one Q5), when that is safe: every tag in the block shares one prefix, the number follows
/// the question before it (or leads up to the one after), is not taken in the block and stays
/// below the next tag. Any other untagged question stays unnumbered, as it was written. Returns
/// the tags it gave.
fn number_untagged(questions: &mut [FormQuestion]) -> Vec<String> {
    let mut prefixes = questions
        .iter()
        .filter(|question| !question.tag.is_empty())
        .map(|question| tag_number(&question.tag).map(|(prefix, _)| prefix.to_string()));
    let Some(Some(prefix)) = prefixes.next() else {
        return Vec::new();
    };
    if prefixes.any(|other| other.as_ref() != Some(&prefix)) {
        return Vec::new();
    }
    let tagged: Vec<(usize, u64)> = questions
        .iter()
        .enumerate()
        .filter_map(|(index, question)| {
            tag_number(&question.tag).map(|(_, number)| (index, number))
        })
        .collect();
    let mut taken: Vec<u64> = tagged.iter().map(|(_, number)| *number).collect();
    let mut given = Vec::new();
    let mut previous: Option<u64> = None;
    for index in 0..questions.len() {
        if let Some((_, number)) = tag_number(&questions[index].tag) {
            previous = Some(number);
            continue;
        }
        let next = tagged.iter().find(|(at, _)| *at > index);
        let candidate = match (previous, next) {
            (Some(number), _) => number.checked_add(1),
            (None, Some((at, number))) => number.checked_sub((at - index) as u64),
            (None, None) => None,
        };
        let safe = candidate.filter(|number| {
            *number > 0 && !taken.contains(number) && next.is_none_or(|(_, next)| number < next)
        });
        previous = safe;
        if let Some(number) = safe {
            taken.push(number);
            questions[index].tag = format!("{prefix}{number}");
            given.push(questions[index].tag.clone());
        }
    }
    given
}

/// Reads one block. A block that cannot be read is still a form, with `error` saying why, so the
/// chat shows the problem in place of the questions.
///
/// CDXC:SessionChat 2026-11-05 DECISION:
/// User (via the coordinator, after a 10.47.0 user's coordinator wrote a bare JSON list of questions and the chat showed "These questions could not be shown"): "Heal such problems in the app by itself, without the user pasting the error back to the agent." This supersedes the 2026-10-17 "one canonical shape" rule (server/src/session_chat_question_resend.rs) for the near-misses whose meaning is plain: a bare list of questions, a single question object, "questions" holding one object, "text" for the title, a missing tag between tagged questions, trailing commas and smart quotes are read as the form they mean. JSON that still cannot be read keeps its error card, and gxserver asks the agent once to send it again. `ghostex visual check` names each near-miss it mended ([`check_form`]) so agents learn the canonical shape.
pub fn parse_form(body: &str) -> QuestionForm {
    check_form(body).0
}

/// [`parse_form`], with the near-misses it read past, each as an instruction to write it the
/// canonical way (`ghostex visual check` prints them).
pub fn check_form(body: &str) -> (QuestionForm, Vec<String>) {
    let id = form_id(body);
    let mut mended: Vec<String> = Vec::new();
    let mut read = || -> Result<QuestionForm, String> {
        let value: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(error) => {
                let value = mend_json(body)
                    .and_then(|fixed| serde_json::from_str(&fixed).ok())
                    .ok_or_else(|| format!("The questions block is not valid JSON: {error}."))?;
                mended.push(
                    "Write plain JSON: straight double quotes (\") and no comma before a closing ] or }."
                        .to_string(),
                );
                value
            }
        };
        let empty = Map::new();
        let (object, list, withdraw) = match &value {
            Value::Array(list) if !list.is_empty() => {
                mended.push(
                    "Wrap the list of questions in one object: {\"questions\": [ … ]}.".to_string(),
                );
                (&empty, list.clone(), Vec::new())
            }
            Value::Object(object) => {
                let withdraw = withdrawn_tags(object.get("withdraw"))?;
                match object.get("questions") {
                    Some(Value::Object(question)) => {
                        mended.push(
                            "\"questions\" is a list, even of one: \"questions\": [{ … }]."
                                .to_string(),
                        );
                        (object, vec![Value::Object(question.clone())], withdraw)
                    }
                    None if !withdraw.is_empty() => (object, Vec::new(), withdraw),
                    None if is_lone_question(object) => {
                        mended.push(
                            "Put even one question in a block: {\"questions\": [{ … }]}."
                                .to_string(),
                        );
                        (&empty, vec![value.clone()], Vec::new())
                    }
                    questions => (
                        object,
                        questions
                            .and_then(Value::as_array)
                            .filter(|list| !list.is_empty())
                            .cloned()
                            .ok_or(
                                "A questions block needs a \"questions\" list with at least one question.",
                            )?,
                        withdraw,
                    ),
                }
            }
            _ => {
                return Err(
                    "A questions block is one JSON object with a \"questions\" list.".to_string(),
                )
            }
        };
        if list.len() > MAX_QUESTIONS {
            return Err(format!(
                "The block asks {} questions; a form takes at most {MAX_QUESTIONS}.",
                list.len()
            ));
        }
        let mut used = Vec::new();
        let mut questions = list
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .as_object()
                    .ok_or_else(|| format!("Question {} must be an object.", index + 1))
                    .and_then(|object| question(object, index + 1, &mut used))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if list.iter().any(|value| {
            value.get("title").is_none()
                && value.get("question").is_none()
                && value.get("text").is_some()
        }) {
            mended.push("Give each question its \"title\" (not \"text\").".to_string());
        }
        let given = number_untagged(&mut questions);
        if !given.is_empty() {
            mended.push(format!(
                "Give every question its \"tag\": {} had none and {} numbered from the questions beside it.",
                given.join(", "),
                if given.len() == 1 { "was" } else { "were" }
            ));
        }
        Ok(QuestionForm {
            id: id.clone(),
            title: Some(string(object.get("title")))
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "Ghostex form".to_string()),
            submit: Some(string(object.get("submit")))
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| "Send answers".to_string()),
            questions,
            withdraw,
            error: String::new(),
            resent: false,
            inline: object.get("inline") == Some(&Value::Bool(true)),
        })
    };
    let form = read().unwrap_or_else(|error| QuestionForm {
        id: id.clone(),
        title: "Ghostex form".to_string(),
        error,
        ..QuestionForm::default()
    });
    if !form.error.is_empty() {
        mended.clear();
    }
    (form, mended)
}

/// The tag that wraps the message Ghostex sends an agent whose ```questions block could not be
/// read; the chat folds it into one line and marks the broken form as asked again.
pub const RESEND_TAG: &str = "ghostex-questions-resend";

/// The message that asks an agent to send a block that could not be read again, in the one shape
/// the form reads (server/src/session_chat_question_resend.rs sends it).
pub fn resend_message(form: &QuestionForm) -> String {
    format!(
        r#"<{RESEND_TAG} form="{id}">
Ghostex could not show the ```questions block in your last reply as a form: {error}

Ask those same questions again, keeping each question's Q-number as its "tag", in one ```questions block of exactly this shape, and do nothing else in this reply (no other work, no other text):

```questions
{{"title": "Open questions",
 "questions": [
   {{"tag": "Q1", "title": "Which layout should I build?", "detail": "Why it matters.",
    "choices": [{{"label": "Tabs", "detail": "One page per area.", "recommended": true}}, {{"label": "One long page"}}]}},
   {{"tag": "Q2", "title": "What should the page be called?", "type": "text"}}
 ]}}
```

The block is one JSON object with a "questions" list. Each question has a "title" and either "choices" (each choice is its text or {{"label", "detail", "recommended"}}) or "type": "text". Before replying, write the block to a file and run `ghostex visual check <file>` until it passes; `ghostex visual --help` has the full grammar.
</{RESEND_TAG}>"#,
        id = form.id,
        error = form.error,
    )
}

/// The id of the form a resend message ([`resend_message`]) asks about, when `text` is one.
pub fn resend_form_id(text: &str) -> Option<&str> {
    let rest = text
        .trim_start()
        .strip_prefix('<')?
        .strip_prefix(RESEND_TAG)?
        .trim_start()
        .strip_prefix("form=\"")?;
    let id = &rest[..rest.find('"')?];
    (!id.is_empty()).then_some(id)
}

/// Whether the form ends with its own notes box: a form of two or more questions does.
pub fn takes_notes(form: &QuestionForm) -> bool {
    form.questions.len() >= 2
}

/// The answers key the form's notes arrive under: [`NOTES_KEY`], numbered like a repeated
/// question's key when a question already answers under it.
pub fn notes_key(form: &QuestionForm) -> String {
    let taken = |key: &str| {
        form.questions
            .iter()
            .any(|question| question.key == key || format!("{}{NOTE_SUFFIX}", question.key) == key)
    };
    let mut key = NOTES_KEY.to_string();
    let mut copy = 2;
    while taken(&key) {
        key = format!("{NOTES_KEY} ({copy})");
        copy += 1;
    }
    key
}

/// The message that carries a form's answers to the agent: the envelope a published page's
/// answers use (`server/src/visual_pages/answers.rs`), so the agent reads both the same way and
/// the chat draws both as an answers card.
pub fn answers_message(form: &QuestionForm, answers: &Value) -> String {
    page_answers_message(&form.id, &form.title, answers)
}

/// The `<ghostex-page-answers page="…" title="…">` message, for a question form or a published page
/// alike (server/src/visual_pages/answers.rs).
///
/// CDXC:SessionChat 2026-10-11 WHY:
/// The agent reads this envelope as text, so the title keeps its own characters ("Interrupt & Send
/// follow-ups", not "&amp;"): only a quote, an angle bracket, a line break and an ampersand that
/// would read as one of these escapes are escaped, which is what the chat's parser
/// (packages/gx-chat-core/src/transcript/page_answers.rs) needs to read the title back exactly.
pub fn page_answers_message(id: &str, title: &str, answers: &Value) -> String {
    let body = serde_json::to_string_pretty(answers).unwrap_or_else(|_| "null".to_string());
    format!(
        "<ghostex-page-answers page=\"{id}\" title=\"{}\">\n{body}\n</ghostex-page-answers>",
        escape_title(title)
    )
}

fn escape_title(title: &str) -> String {
    let mut escaped = String::with_capacity(title.len());
    for (at, character) in title.char_indices() {
        match character {
            '&' if ["&amp;", "&quot;", "&lt;", "&gt;"]
                .iter()
                .any(|entity| title[at..].starts_with(entity)) =>
            {
                escaped.push_str("&amp;")
            }
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\r' | '\n' => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped
}
