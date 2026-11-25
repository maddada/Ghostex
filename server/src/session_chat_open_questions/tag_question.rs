//! One question of a session's chat found by its tag: the latest ```questions form, one-click
//! question or Live page in the transcript that asks it, with its words, its choices, how it
//! stands and the answer that came back. `ghostex question show` and the quote under a message
//! that points at a question read it (server/src/chat_item_quotes/question.rs).

use ghostex_gx_chat_core::transcript::open_questions::{AskerKind, Found, TagState};
use ghostex_gx_chat_core::transcript::page_answers::split_all_page_answers;
use serde_json::Value;

use crate::domain::DomainRepository;
use crate::server::AppState;

/// A question the chat asked, as its form or page holds it.
pub(crate) struct ChatQuestion {
    /// The form's id or the page's id.
    pub(crate) asker_id: String,
    pub(crate) on_page: bool,
    /// The page's title, for a page's question.
    pub(crate) page_title: Option<String>,
    /// When the reply that asks it arrived (epoch ms), if the transcript says.
    pub(crate) asked_at_ms: Option<i64>,
    /// How its tag stands in the chat; `None` when the ledger does not say.
    pub(crate) state: Option<TagState>,
    pub(crate) title: String,
    pub(crate) detail: String,
    /// Each choice's label and whether it is the recommended one.
    pub(crate) choices: Vec<(String, bool)>,
    /// The answer sent for it from the form or page, as sent.
    pub(crate) answer: Option<Value>,
}

/// The question tagged `tag` in the chat of `project_id`/`session_id`, read up to date first.
/// `None` when no form or page there asks it. Blocking: it may read the transcript.
pub(crate) fn chat_question(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    tag: &str,
) -> Option<ChatQuestion> {
    let repository = DomainRepository::new(db, state.metadata.server_id.as_str());
    let session = repository.get_session(project_id, session_id).ok().flatten()?;
    let _ = super::read_open(state, db, &session, false);
    let queued = super::queued_answers(db, project_id, session_id);
    let ledger_key = crate::server::session_observer_key(project_id, session_id);
    let entry = super::ledgers().lock().ok()?.get(&ledger_key).cloned()?;
    let (asker, form, tag_state) = {
        let ledger = entry.lock().ok()?;
        let asker = ledger.found.iter().rev().find_map(|found| match found {
            Found::Asker(asker) if asker.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) => {
                Some(asker.clone())
            }
            _ => None,
        })?;
        let form = ledger.forms.get(&asker.id).cloned();
        let states = ledger.tag_outcomes(&queued, |id| {
            crate::visual_pages::page_answered(&state.paths.root_dir, id)
        });
        (asker, form, states.get(&tag.to_ascii_uppercase()).copied())
    };
    let mut found = ChatQuestion {
        asker_id: asker.id.clone(),
        on_page: asker.kind == AskerKind::Page,
        page_title: None,
        asked_at_ms: asker.at,
        state: tag_state,
        title: String::new(),
        detail: String::new(),
        choices: Vec::new(),
        answer: None,
    };
    if found.on_page {
        let page = crate::visual_pages::page_overview(&state.paths.root_dir, &asker.id)?;
        let question = page
            .questions
            .iter()
            .find(|question| question.tag.trim().eq_ignore_ascii_case(tag))?;
        found.title = question.title.clone();
        found.choices = question
            .choices
            .iter()
            .map(|choice| (choice.label.clone(), choice.recommended))
            .collect();
        found.answer = page
            .answers
            .get(&question.key)
            .filter(|answer| !answer.is_null())
            .cloned();
        found.page_title = Some(page.title);
        return Some(found);
    }
    let form = form?;
    let question = form
        .questions
        .iter()
        .find(|question| question.tag.trim().eq_ignore_ascii_case(tag))?;
    found.title = question.title.clone();
    found.detail = question.detail.clone();
    found.choices = question
        .choices
        .iter()
        .map(|choice| (choice.label.clone(), choice.recommended))
        .collect();
    if tag_state == Some(TagState::Answered) || tag_state == Some(TagState::Skipped) {
        found.answer = form_answer(&repository, db, project_id, session_id, &form.id, &question.key);
    }
    Some(found)
}

/// The answer the form `form_id` sent for its question `key`, read from the message that carried
/// the form's answers (the session's resource index keeps where it is). `None` when it was answered
/// another way (in the user's own words) or the transcript no longer has it.
fn form_answer(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    form_id: &str,
    key: &str,
) -> Option<Value> {
    let read =
        crate::session_chat_resources::read_resource(repository, db, project_id, session_id, form_id)
            .ok()?;
    let message = read["messages"].as_array()?.get(1)?;
    let text: Vec<&str> = message["blocks"]
        .as_array()?
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    let (envelopes, _) = split_all_page_answers(&text.join("\n\n"));
    envelopes
        .into_iter()
        .find(|answers| answers.page_id == form_id)?
        .answers
        .get(key)
        .filter(|answer| !answer.is_null())
        .cloned()
}
