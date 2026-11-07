//! The questions an agent asked in its chat, for the voice agent: every open ```questions form and
//! question page (`ghostex show`) of a session, listed with a ref for each question (its tag, like
//! Q335) and answered the way the reader would: a form's answers go into the session's chat queue as
//! the same `<ghostex-page-answers>` message the chat's Send answers sends (built by gx-chat-core's
//! own `forms::answers`), and a page's answers go through the page's own answers route, so the
//! agent cannot tell who answered and the chat marks the form or page Answered.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User (as the voice agent): "I tried to answer your inline question Q335 using the answer tool, but it returned that nothing was waiting for an answer ... Please adjust ... so the voice agent can submit an answer to them directly." and then "When I tried to answer your open question Q351 by choosing the recommended option (add both hotkeys as proposed), the answer came through as skipped instead of selected ... Please fix the question/answer handling so a chosen option actually registers next time you ask one." get_pending_prompt lists a session's open forms and question pages (the same ones its pink question dot counts), and answer_prompt answers them by each question's ref. A pick registers by option number, letter, label (exact, any case, with or without its detail or "recommended" badge, or a clear prefix) or "recommended". A question named with no usable answer refuses the whole call with its valid choices and sends nothing; a question goes as skipped only when the voice agent says skip, or when it answered another question of the same form and left this one out (like a blank question when the reader presses Send answers). Forms and pages the user did not touch stay open. The result says exactly what was recorded ("Q351: Add both as proposed") so the voice agent can tell the user. This supersedes the first version, which turned any pick it could not read into skipped.
//! SEE-ALSO: packages/gx-chat-core/src/questions/forms.rs (`answers`, the form's keys), server/src/visual_pages/kit.rs (the page's keys and values), server/src/visual_pages/questions.rs, server/src/session_chat_open_questions.rs.

use std::sync::Arc;
use std::time::Duration;

use ghostex_gx_chat_core::questions::forms::answers as form_answers;
use ghostex_gx_chat_core::transcript::open_questions::AskerKind;
use ghostex_gx_chat_core::transcript::question_forms::{
    answers_message, takes_notes, QuestionForm, NOTE_SUFFIX, OTHER_CHOICE, SKIPPED,
};
use ghostex_gx_chat_core::QuestionDraft;
use serde_json::{json, Map, Value};

use super::loopback;
use super::sessions::SessionInfo;
use crate::server::AppState;

/// One choice as the voice agent hears it.
struct Choice {
    label: String,
    detail: String,
    /// What a page's answers carry for it (a form's are built by gx-chat-core).
    value: String,
    recommended: bool,
}

struct Question {
    /// What the voice agent names it by: its tag, else "<form>.<question>".
    reference: String,
    /// The page's answers key (unused for a form, whose keys gx-chat-core builds).
    key: String,
    title: String,
    detail: String,
    multiple: bool,
    text: bool,
    choices: Vec<Choice>,
}

/// One open form or question page.
struct Asked {
    id: String,
    kind: AskerKind,
    title: String,
    form: Option<QuestionForm>,
    questions: Vec<Question>,
}

/// The session's open forms and question pages, oldest first. Blocking.
fn collect(state: &AppState, project_id: &str, session_id: &str) -> Vec<Asked> {
    let mut asked: Vec<Asked> = Vec::new();
    for asker in crate::session_chat_open_questions::open_askers(state, project_id, session_id) {
        match (&asker.kind, asker.form) {
            (AskerKind::Form, Some(form)) => asked.push(Asked {
                id: asker.id,
                kind: AskerKind::Form,
                title: form.title.clone(),
                questions: form
                    .questions
                    .iter()
                    .map(|question| Question {
                        reference: question.tag.clone(),
                        key: question.key.clone(),
                        title: question.title.clone(),
                        detail: question.detail.clone(),
                        multiple: question.multiple,
                        text: question.text,
                        choices: question
                            .choices
                            .iter()
                            .map(|choice| Choice {
                                label: choice.label.clone(),
                                detail: choice.detail.clone(),
                                value: choice.value.clone(),
                                recommended: choice.recommended,
                            })
                            .collect(),
                    })
                    .collect(),
                form: Some(form),
            }),
            (AskerKind::Page, _) => {
                let Some((title, questions)) =
                    crate::visual_pages::page_questions(&state.paths.root_dir, &asker.id)
                else {
                    continue;
                };
                asked.push(Asked {
                    id: asker.id,
                    kind: AskerKind::Page,
                    title,
                    form: None,
                    questions: questions
                        .into_iter()
                        .map(|question| Question {
                            reference: question.tag,
                            key: question.key,
                            title: question.title,
                            detail: String::new(),
                            multiple: question.multiple,
                            text: question.text,
                            choices: question
                                .choices
                                .into_iter()
                                .map(|choice| Choice {
                                    label: choice.label,
                                    detail: String::new(),
                                    value: choice.value,
                                    recommended: choice.recommended,
                                })
                                .collect(),
                        })
                        .collect(),
                });
            }
            _ => {}
        }
    }
    // A tag names its question only when no other open question uses it.
    let tags: Vec<String> = asked
        .iter()
        .flat_map(|asked| asked.questions.iter())
        .map(|question| question.reference.to_ascii_uppercase())
        .collect();
    for (number, entry) in asked.iter_mut().enumerate() {
        for (index, question) in entry.questions.iter_mut().enumerate() {
            let tag = question.reference.trim().to_ascii_uppercase();
            if tag.is_empty() || tags.iter().filter(|known| **known == tag).count() > 1 {
                question.reference = format!("{}.{}", number + 1, index + 1);
            }
        }
    }
    asked
}

/// The open forms and pages of `session`. Those of a session on another computer are in that
/// computer's database and transcripts, which no endpoint another computer may use reads.
async fn collect_async(state: &Arc<AppState>, session: &SessionInfo) -> Vec<Asked> {
    if session.computer.is_some() {
        return Vec::new();
    }
    let state = state.clone();
    let (project_id, session_id) = (session.project_id.clone(), session.session_id.clone());
    tokio::task::spawn_blocking(move || collect(&state, &project_id, &session_id))
        .await
        .unwrap_or_default()
}

fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// The session's open forms and question pages, for get_pending_prompt. Empty when none wait.
pub(super) async fn listing(state: &Arc<AppState>, session: &SessionInfo) -> Vec<Value> {
    collect_async(state, session)
        .await
        .iter()
        .enumerate()
        .map(|(number, asked)| {
            json!({
                "number": number + 1,
                "kind": if asked.kind == AskerKind::Page { "page" } else { "form" },
                "title": short(&asked.title, 200),
                "questions": asked.questions.iter().map(|question| {
                    let mut entry = json!({
                        "ref": question.reference,
                        "question": short(&question.title, 400),
                        "kind": if question.text { "text" } else if question.multiple { "several choices" } else { "one choice" },
                    });
                    if !question.detail.is_empty() {
                        entry["detail"] = json!(short(&question.detail, 300));
                    }
                    if !question.text {
                        entry["options"] = json!(question.choices.iter().enumerate().map(|(n, choice)| {
                            let mut option = json!({"number": n + 1, "label": short(&choice.label, 200)});
                            if !choice.detail.is_empty() {
                                option["detail"] = json!(short(&choice.detail, 200));
                            }
                            if choice.recommended {
                                option["recommended"] = json!(true);
                            }
                            option
                        }).collect::<Vec<_>>());
                    }
                    entry
                }).collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// The fields a question's ref may come in: `question` is the documented one.
const REF_FIELDS: [&str; 3] = ["question", "ref", "tag"];

fn question_ref(entry: &Value) -> String {
    REF_FIELDS
        .iter()
        .map(|field| text_arg(entry, field))
        .find(|wanted| !wanted.is_empty())
        .unwrap_or_default()
}

/// Whether the call names questions by ref, so it is meant for a form or page.
pub(super) fn names_questions(args: &Value) -> bool {
    !question_ref(args).is_empty()
        || args
            .get("answers")
            .and_then(Value::as_array)
            .is_some_and(|answers| {
                answers
                    .iter()
                    .any(|answer| !question_ref(answer).is_empty())
            })
}

/// What the user said for one question.
#[derive(Default)]
struct Given {
    options: Vec<usize>,
    /// A free answer: a text question's answer, or "Other / none of these" with this note.
    text: String,
    note: String,
    skip: bool,
}

fn text_arg(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// The fields a voice model puts a pick in: `options` is the documented one, the rest are the
/// names models reach for anyway. Each holds a number, a string or a list of them.
const PICK_FIELDS: [&str; 6] = ["options", "option", "choice", "choices", "answer", "label"];

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .trim_matches(|c: char| c.is_whitespace() || "\"'`.!?".contains(c))
        .to_string()
}

fn says_skip(text: &str) -> bool {
    matches!(normalized(text).as_str(), "skip" | "skipped" | "skip it")
}

/// The question's options as the voice agent can quote them back.
fn choices_list(question: &Question) -> String {
    question
        .choices
        .iter()
        .enumerate()
        .map(|(n, choice)| {
            let badge = if choice.recommended {
                " (recommended)"
            } else {
                ""
            };
            format!("{}. {}{badge}", n + 1, choice.label)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The option an exact label names, ignoring case and a "(recommended)" badge.
fn exact_label(question: &Question, said: &str) -> Option<usize> {
    let said = normalized(said);
    let bare = normalized(&said.replace("(recommended)", "").replace("recommended", ""));
    question.choices.iter().position(|choice| {
        let label = normalized(&choice.label);
        !label.is_empty() && (label == said || label == bare)
    })
}

/// The options one spoken pick names: an option number (1, 2, 3), its letter (a, b, c), its
/// label (exact or a clear prefix, with or without its detail or "recommended" badge), or
/// "recommended". `None` when it names none.
fn resolve_pick(question: &Question, said: &str) -> Option<Vec<usize>> {
    let count = question.choices.len();
    let text = normalized(said);
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let filler = ["the", "one", "option", "choice", "answer", "pick", "number"];
    let meant: Vec<&str> = words
        .iter()
        .copied()
        .filter(|word| !filler.contains(word))
        .collect();
    if meant == ["recommended"] {
        let recommended: Vec<usize> = (0..count)
            .filter(|index| question.choices[*index].recommended)
            .collect();
        return (!recommended.is_empty()).then_some(recommended);
    }
    if let [only] = meant.as_slice() {
        let ordinals = ["first", "second", "third", "fourth", "fifth", "sixth"];
        let index = if let Ok(number) = only.parse::<usize>() {
            number.checked_sub(1)
        } else if only.len() == 1 && only.as_bytes()[0].is_ascii_lowercase() {
            Some((only.as_bytes()[0] - b'a') as usize)
        } else if *only == "last" {
            count.checked_sub(1)
        } else {
            ordinals.iter().position(|ordinal| ordinal == only)
        };
        // A bare number, letter or ordinal names an option only when the form has that many.
        if let Some(index) = index.filter(|index| *index < count) {
            if only.parse::<usize>().is_ok()
                || only.len() == 1
                || exact_label(question, said).is_none()
            {
                return Some(vec![index]);
            }
        }
    }
    if let Some(index) = exact_label(question, said) {
        return Some(vec![index]);
    }
    let bare = normalized(&text.replace("(recommended)", ""));
    let unique = |matches: Vec<usize>| (matches.len() == 1).then_some(matches);
    // The label with its detail or a badge after it ("Add both as proposed: ⌃⌥V …").
    let mut longer: Vec<(usize, usize)> = question
        .choices
        .iter()
        .enumerate()
        .filter_map(|(index, choice)| {
            let label = normalized(&choice.label);
            let rest = bare.strip_prefix(&label)?;
            (!label.is_empty() && rest.starts_with(|c: char| !c.is_alphanumeric()))
                .then_some((index, label.len()))
        })
        .collect();
    longer.sort_by_key(|(_, length)| std::cmp::Reverse(*length));
    if let Some((index, length)) = longer.first() {
        if longer.get(1).is_none_or(|(_, next)| next < length) {
            return Some(vec![*index]);
        }
    }
    // The start of a label ("add both").
    if bare.chars().count() >= 3 {
        return unique(
            question
                .choices
                .iter()
                .enumerate()
                .filter(|(_, choice)| normalized(&choice.label).starts_with(&bare))
                .map(|(index, _)| index)
                .collect(),
        );
    }
    None
}

/// What one answers entry says for its question. Refuses an entry that names the question but
/// gives no usable answer, listing the valid choices, so nothing is sent as skipped by mistake.
fn given_for(
    question: &Question,
    entry: &Value,
    fallback: Option<&Value>,
) -> Result<Given, String> {
    let mut picks: Vec<String> = Vec::new();
    let mut skip = entry.get("skip").and_then(Value::as_bool).unwrap_or(false);
    let mut push = |value: &Value| match value {
        Value::Number(number) => picks.push(number.to_string()),
        Value::String(text) if says_skip(text) => skip = true,
        Value::String(text) if !text.trim().is_empty() => picks.push(text.clone()),
        _ => {}
    };
    let mut named = false;
    for field in PICK_FIELDS {
        match entry.get(field) {
            Some(Value::Array(values)) => values.iter().for_each(&mut push),
            Some(value) => push(value),
            None => continue,
        }
        named = true;
    }
    if !named {
        if let Some(value) = fallback {
            push(value);
        }
    }
    let mut text = text_arg(entry, "text");
    if says_skip(&text) {
        skip = true;
        text.clear();
    }
    let note = text_arg(entry, "note");
    let unusable = |said: &str| {
        let how = if question.text {
            "This question takes a free answer in text.".to_string()
        } else {
            format!(
                "Its options: {}. Give an option number, letter or label, or \"recommended\".",
                choices_list(question)
            )
        };
        format!(
            "{}: {said} Nothing was sent. {how} To skip it on purpose, give skip: true.",
            question.reference
        )
    };
    if skip {
        return Ok(Given {
            skip: true,
            note,
            ..Given::default()
        });
    }
    let mut options: Vec<usize> = Vec::new();
    if question.text {
        // A text question's answer may come in any of the pick fields.
        if text.is_empty() {
            text = picks.join(", ");
        }
    } else {
        for said in &picks {
            let Some(found) = resolve_pick(question, said) else {
                return Err(unusable(&format!("\"{said}\" is not one of its options.")));
            };
            for index in found {
                if !options.contains(&index) {
                    options.push(index);
                }
            }
        }
        // A free answer that is word for word an option is that option, not "Other".
        if let Some(index) = exact_label(question, &text) {
            if !options.contains(&index) {
                options.push(index);
            }
            text.clear();
        }
        options.sort_unstable();
        if options.len() > 1 && !question.multiple {
            return Err(unusable("It takes one option, but several were given."));
        }
    }
    if options.is_empty() && text.is_empty() && note.is_empty() {
        return Err(unusable("No answer was given for it."));
    }
    Ok(Given {
        options,
        text,
        note,
        skip: false,
    })
}

fn draft(question: &Question, given: Option<&Given>) -> QuestionDraft {
    let mut draft = QuestionDraft::default();
    let Some(given) = given else {
        draft.skipped = true;
        return draft;
    };
    draft.indices = given.options.iter().map(|index| *index as u32).collect();
    if question.text {
        draft.other = if given.text.is_empty() {
            given.note.clone()
        } else {
            given.text.clone()
        };
    } else if !given.text.is_empty() {
        draft.other_picked = true;
        draft.other = given.text.clone();
    } else {
        draft.other = given.note.clone();
    }
    // A question with nothing given goes as skipped, like a blank one in the chat's form.
    draft.skipped = given.skip
        || (draft.indices.is_empty() && !draft.other_picked && draft.other.trim().is_empty());
    draft
}

/// A page's answers, keyed and valued like the page kit's `collect` (kit.rs).
fn page_answers(asked: &Asked, given: &[Option<&Given>]) -> Value {
    let mut out = Map::new();
    for (question, given) in asked.questions.iter().zip(given) {
        let given = given.filter(|given| !given.skip);
        if question.text {
            let answer = given
                .map(|given| {
                    if given.text.is_empty() {
                        given.note.clone()
                    } else {
                        given.text.clone()
                    }
                })
                .filter(|answer| !answer.is_empty());
            out.insert(
                question.key.clone(),
                json!(answer.unwrap_or_else(|| SKIPPED.to_string())),
            );
            continue;
        }
        let Some(given) = given else {
            out.insert(question.key.clone(), json!(SKIPPED));
            continue;
        };
        let mut picked: Vec<Value> = given
            .options
            .iter()
            .filter_map(|index| question.choices.get(*index))
            .map(|choice| json!(choice.value))
            .collect();
        let mut note = given.note.clone();
        if !given.text.is_empty() {
            picked.push(json!(OTHER_CHOICE));
            note = given.text.clone();
        }
        let value = match (picked.is_empty(), question.multiple) {
            (false, true) => Value::Array(picked),
            (false, false) => picked.remove(0),
            (true, _) if !note.is_empty() => Value::Null,
            (true, _) => json!(SKIPPED),
        };
        out.insert(question.key.clone(), value);
        if !note.is_empty() {
            out.insert(format!("{}{NOTE_SUFFIX}", question.key), json!(note));
        }
    }
    Value::Object(out)
}

/// Answers questions of the session's open forms and pages by ref. Each form or page that gets an
/// answer is sent, its other questions as skipped; the others stay open.
pub(super) async fn answer(
    state: &Arc<AppState>,
    session: &SessionInfo,
    args: &Value,
) -> Result<Value, String> {
    if let Some(computer) = &session.computer {
        return Err(format!("Ghostex forms and Ghostex pages in a chat on {} can't be answered from here, so nothing was sent. The user can answer them in Ghostex.", computer.name));
    }
    let asked = collect_async(state, session).await;
    if asked.is_empty() {
        return Err("Nothing is waiting for an answer in that session any more.".to_string());
    }
    let refs = || {
        asked
            .iter()
            .flat_map(|asked| {
                asked
                    .questions
                    .iter()
                    .map(|question| question.reference.clone())
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    // (form or page, question) -> what the user said.
    let mut given: Vec<Vec<Option<Given>>> = asked
        .iter()
        .map(|asked| asked.questions.iter().map(|_| None).collect())
        .collect();
    let mut entries: Vec<Value> = args
        .get("answers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // A pick given at the top level (`option`, `choice`, …) with no answers entry names the
    // question it gives or, when that is left out, the one question waiting.
    let only_question: Vec<&Question> = asked.iter().flat_map(|asked| &asked.questions).collect();
    let top_level_pick = PICK_FIELDS
        .iter()
        .chain(&["text", "skip"])
        .any(|field| args.get(*field).is_some());
    if entries.is_empty() && top_level_pick {
        let wanted = question_ref(args);
        let wanted = match (wanted.is_empty(), only_question.as_slice()) {
            (false, _) => wanted,
            (true, [only]) => only.reference.clone(),
            (true, _) => {
                return Err(format!(
                    "Several questions are open; give one answers entry per question with its ref. Open questions: {}.",
                    refs()
                ))
            }
        };
        let mut entry = args.clone();
        entry["question"] = json!(wanted);
        entries.push(entry);
    }
    let answer_count = entries.len();
    for entry in &entries {
        let wanted = question_ref(entry);
        let found = asked.iter().enumerate().find_map(|(a, asked)| {
            asked
                .questions
                .iter()
                .position(|question| question.reference.eq_ignore_ascii_case(&wanted))
                .map(|q| (a, q))
        });
        let Some((a, q)) = found else {
            return Err(format!(
                "No open question is called \"{wanted}\". Open questions: {}.",
                refs()
            ));
        };
        let question = &asked[a].questions[q];
        // A lone entry may carry its pick in the call's top-level `option`.
        let fallback = (answer_count == 1).then(|| args.get("option")).flatten();
        given[a][q] = Some(given_for(question, entry, fallback)?);
    }
    let notes = text_arg(args, "notes");
    let mut sent: Vec<Value> = Vec::new();
    let mut recorded: Vec<String> = Vec::new();
    for (asked, given) in asked.iter().zip(&given) {
        if given.iter().all(Option::is_none) {
            continue;
        }
        let picks = |question: &Question, given: &Option<Given>| -> Vec<String> {
            match given {
                None => vec!["skipped".to_string()],
                Some(given) if given.skip => vec!["skipped".to_string()],
                Some(given) => {
                    let mut said: Vec<String> = given
                        .options
                        .iter()
                        .filter_map(|index| question.choices.get(*index))
                        .map(|choice| choice.label.clone())
                        .collect();
                    if !given.text.is_empty() {
                        said.push(if question.text {
                            given.text.clone()
                        } else {
                            format!("Other ({})", given.text)
                        });
                    }
                    if said.is_empty() {
                        said.push("no option, only a note".to_string());
                    }
                    if !given.note.is_empty() && !question.text {
                        said.push(format!("note: {}", given.note));
                    }
                    said
                }
            }
        };
        let said: Vec<String> = asked
            .questions
            .iter()
            .zip(given)
            .map(|(question, given)| {
                format!(
                    "{}: {}",
                    question.reference,
                    picks(question, given).join(", ")
                )
            })
            .collect();
        recorded.extend(said.iter().cloned());
        match (&asked.kind, &asked.form) {
            (AskerKind::Form, Some(form)) => {
                let drafts: Vec<QuestionDraft> = asked
                    .questions
                    .iter()
                    .zip(given)
                    .map(|(question, given)| draft(question, given.as_ref()))
                    .collect();
                let notes = if takes_notes(form) {
                    notes.as_str()
                } else {
                    ""
                };
                let answers = form_answers(form, &drafts, notes);
                let millis = chrono::Utc::now().timestamp_millis();
                // The chat's Send answers queues the same message (gx-chat-core questions/forms.rs `send`).
                loopback::call(
                    state,
                    "/api/queueSessionChatPrompt",
                    json!({
                        "projectId": session.project_id,
                        "sessionId": session.session_id,
                        "text": answers_message(form, &answers),
                        "sendRequestId": format!("question-form-{}-{millis}", form.id),
                    }),
                    Duration::from_secs(30),
                )
                .await?;
            }
            _ => {
                let given: Vec<Option<&Given>> = given.iter().map(Option::as_ref).collect();
                let body = json!({"answers": page_answers(asked, &given)}).to_string();
                let routed = crate::visual_pages::submit_answers(
                    state.clone(),
                    format!(
                        "{}{}/answers",
                        crate::visual_pages::VISUAL_PAGE_ROUTE_PREFIX,
                        asked.id
                    ),
                    axum::body::Body::from(body),
                )
                .await;
                let status = routed.response.status();
                let reply = axum::body::to_bytes(routed.response.into_body(), 64 * 1024)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .unwrap_or(Value::Null);
                if !status.is_success() || reply.get("ok").and_then(Value::as_bool) != Some(true) {
                    return Err(reply
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("The page did not take the answers.")
                        .to_string());
                }
            }
        }
        sent.push(json!({"title": short(&asked.title, 200), "answers": said}));
    }
    if sent.is_empty() {
        return Err(format!(
            "Say which questions to answer by ref. Open questions: {}.",
            refs()
        ));
    }
    // The pink question dot follows at once instead of at the next sweep.
    let refresh_state = state.clone();
    let (project_id, session_id) = (session.project_id.clone(), session.session_id.clone());
    let _ = tokio::task::spawn_blocking(move || {
        crate::session_chat_open_questions::refresh(&refresh_state, &project_id, &session_id, None)
    })
    .await;
    Ok(json!({
        "ok": true,
        "answered": true,
        "session": session.brief(),
        "recorded": recorded.join("; "),
        "sent": sent,
        "note": "Tell the user exactly what was recorded, question by question.",
    }))
}

/// What Autopilot found waiting in a session's open forms and pages.
pub(crate) enum AutopilotForms {
    /// Nothing open.
    None,
    /// Every question has a recommended choice: answered with them (the questions, as recorded).
    Answered(Vec<String>),
    /// Some questions have no recommended choice (or take typed text): nothing was sent; these
    /// are the questions with their choices, for the supervisor.
    NeedsJudgment(Vec<String>),
}

/// Autopilot's answer to a session's open Ghostex forms and pages: each question's recommended
/// choice, the same way the voice agent's "recommended" pick answers it, or nothing when any
/// question lacks one.
pub(crate) async fn autopilot_answer_recommended(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
) -> Result<AutopilotForms, String> {
    let session = SessionInfo {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        title: String::new(),
        project: String::new(),
        agent: String::new(),
        state: super::sessions::VoiceState::WaitingOnQuestion,
        waiting: None,
        last_active_at: String::new(),
        turn_event: None,
        has_chat: true,
        thread_of: None,
        threads: None,
        computer: None,
    };
    let asked = collect_async(state, &session).await;
    if asked.is_empty() {
        return Ok(AutopilotForms::None);
    }
    let lacking: Vec<String> = asked
        .iter()
        .flat_map(|asked| &asked.questions)
        .filter(|question| {
            question.text || !question.choices.iter().any(|choice| choice.recommended)
        })
        .map(|question| {
            format!(
                "{}: {}{} (choices: {})",
                question.reference,
                question.title,
                if question.detail.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", question.detail)
                },
                choices_list(question)
            )
        })
        .collect();
    if !lacking.is_empty() {
        return Ok(AutopilotForms::NeedsJudgment(lacking));
    }
    let answers: Vec<Value> = asked
        .iter()
        .flat_map(|asked| &asked.questions)
        .map(|question| json!({ "question": question.reference, "option": "recommended" }))
        .collect();
    let picked: Vec<String> = asked
        .iter()
        .flat_map(|asked| &asked.questions)
        .map(|question| {
            let labels: Vec<&str> = question
                .choices
                .iter()
                .filter(|choice| choice.recommended)
                .map(|choice| choice.label.as_str())
                .collect();
            format!("{}: {}", question.reference, labels.join(", "))
        })
        .collect();
    answer(state, &session, &json!({ "answers": answers })).await?;
    Ok(AutopilotForms::Answered(picked))
}

/// The supervisor's answer to a session's open forms: one pick per question, in order (an option
/// label, its number or letter, or text for a question that takes text).
pub(crate) async fn autopilot_answer_with(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
    picks: &[String],
) -> Result<bool, String> {
    let session = SessionInfo {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        title: String::new(),
        project: String::new(),
        agent: String::new(),
        state: super::sessions::VoiceState::WaitingOnQuestion,
        waiting: None,
        last_active_at: String::new(),
        turn_event: None,
        has_chat: true,
        thread_of: None,
        threads: None,
        computer: None,
    };
    let asked = collect_async(state, &session).await;
    let questions: Vec<&Question> = asked.iter().flat_map(|asked| &asked.questions).collect();
    if questions.is_empty() {
        return Ok(false);
    }
    let answers: Vec<Value> = questions
        .iter()
        .zip(picks)
        .map(|(question, pick)| {
            if question.text {
                json!({ "question": question.reference, "text": pick })
            } else {
                json!({ "question": question.reference, "option": pick })
            }
        })
        .collect();
    answer(state, &session, &json!({ "answers": answers })).await?;
    Ok(true)
}
