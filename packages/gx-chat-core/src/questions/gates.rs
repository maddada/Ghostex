//! The predicates the question and notice surfaces share.
//!
//! These are the small helper functions `native-host.ts` kept beside `publish` (`noticeVisible`,
//! `asyncQuestionsCanSend`, the `questionCardVisible` expression), collected here because both
//! the document and the action handlers ask the same questions.

use crate::document::QuestionDraft;
use crate::questions::document::blank_drafts;
use crate::questions::interactive::card_dismiss_key;
use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::questions::notice_state::{is_notice_dismissed, notice_dismiss_key};
use crate::state::ChatState;

/// The whole prompt's identity, which is what the saved answers are keyed by.
///
/// A prompt whose text changed is a different card even when its dismiss key is the same, so this
/// is the full value rather than the summary `card_dismiss_key` builds.
pub fn content_key(state: &ChatState) -> Option<String> {
    let prompt = state.session.prompt.as_ref()?;
    Some(format!(
        "interactive:{}",
        serde_json::to_string(prompt).unwrap_or_default()
    ))
}

/// The card's identity, or `None` when nothing is blocking.
pub fn prompt_key(prompt: Option<&InteractivePrompt>) -> Option<String> {
    card_dismiss_key(prompt)
}

/// Whether the notice card is on screen: it exists, its picker has not been answered, and it is
/// not the detection the user closed.
pub fn notice_visible(state: &ChatState) -> bool {
    let Some(notice) = TerminalNotice::parse(state.session.terminal_notice.as_ref()) else {
        return false;
    };
    // An answered approval stays drawn, dimmed, until its prompt is gone (pending_approval.rs).
    if crate::questions::pending_approval::holds_notice(state) {
        return true;
    }
    let questions = &state.questions;
    let key = notice_dismiss_key(Some(&notice));
    if key == questions.answered_notice_key || key == questions.released_notice_key {
        return false;
    }
    if notice.kind == "streamError"
        && crate::transcript::api_errors::card_covers_stream_notice(state)
    {
        return false;
    }
    // CDXC:SessionChat 2026-10-17 WHY: a question an agent asks through a tool reaches the chat twice, as its question card and as the live terminal card of the menu that tool draws (Pi's `cursor_ask_question` showed "Pi is waiting for a choice" above "Needs your answer"; agent flow tests, 2026-10-17). The question card answers that same menu, so the live terminal card steps aside while it stands.
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    if notice.passthrough
        && prompt.as_ref().is_some_and(InteractivePrompt::is_question)
        && question_card_visible(state, prompt.as_ref(), Some(&notice))
    {
        return false;
    }
    !is_notice_dismissed(&notice, questions.dismissed_notice.as_ref())
}

/// Whether a notice is holding the agent's input line, so a message sent now would land in it.
pub fn terminal_choice_pending(state: &ChatState, notice: Option<&TerminalNotice>) -> bool {
    let Some(notice) = notice else { return false };
    notice.holds_input() && notice_dismiss_key(Some(notice)) != state.questions.retired_notice_key
}

/// Whether the question card is drawn.
///
/// An approval is hidden while a live dialog or pass-through notice owns the agent's input,
/// so the two never offer competing ways to answer the same wait.
pub fn question_card_visible(
    state: &ChatState,
    prompt: Option<&InteractivePrompt>,
    notice: Option<&TerminalNotice>,
) -> bool {
    if crate::questions::pending_approval::holds_prompt(state) {
        return true;
    }
    let questions = &state.questions;
    let key = prompt_key(prompt);
    if key.is_none() || key == questions.dismissed_prompt {
        return false;
    }
    let approval = matches!(prompt, Some(InteractivePrompt::Approval { .. }));
    // CDXC:SessionChat 2026-09-26 WHY: a screen dialog with rows or a pass-through screen owns the agent's input, so an approval card beside it offers keys that cannot reach the approval; ExitPlanMode drew its hook card (the plan as JSON, Allow and Deny) under the "Ready to code?" card. The screen's own card is the one card, including while a typed draft makes its approval rows unanswerable.
    let notice_owns_input = notice.is_some_and(|notice| {
        (notice.passthrough
            || notice.choices.as_ref().is_some_and(|rows| !rows.is_empty())
            || notice.dialog.is_some())
            && notice_dismiss_key(Some(notice)) != questions.retired_notice_key
    });
    !(approval && notice_owns_input)
}

/// Whether the async strip may send at all.
pub fn async_questions_can_send(
    state: &ChatState,
    prompt: Option<&InteractivePrompt>,
    notice: Option<&TerminalNotice>,
) -> bool {
    let blocking_question = prompt.is_some_and(InteractivePrompt::is_question)
        && prompt_key(prompt) != state.questions.dismissed_prompt;
    let status = state.session.server_status.as_str();
    !terminal_choice_pending(state, notice)
        && !option_switching(state)
        && !account_busy(state)
        && !blocking_question
        && status != "error"
        && status != "loading"
}

// `optionSwitching` and `accountStatus.busy` are family e's, and family d asks the same two
// questions of the same fields; one pair of predicates answers both.
use crate::composer::document::{account_busy, option_switching};

/// Whether the turn is live, which is the document's `working` key.
///
/// Family a derives it, lifecycle settle and all; the async strip only reports it.
pub fn working(state: &ChatState) -> bool {
    crate::session::working::is_working(state)
}

/// What the card shows for the prompt on screen.
///
/// The stored index and drafts only apply to the prompt they were read for: a card whose content
/// key has moved on draws as if it had just opened, which is what `publish` does before it
/// re-reads the saved answers.
pub struct CardProjection {
    pub index: usize,
    pub drafts: Vec<QuestionDraft>,
    pub answering: bool,
    pub transition: bool,
    pub loading: bool,
}

/// The card projection for the current prompt.
pub fn card_projection(state: &ChatState, prompt: Option<&InteractivePrompt>) -> CardProjection {
    let questions = &state.questions;
    if questions.question_content_key == content_key(state) {
        return CardProjection {
            index: questions.question_index,
            drafts: questions.question_drafts.clone(),
            answering: questions.answering,
            transition: questions.question_transition,
            loading: questions.question_drafts_loading,
        };
    }
    CardProjection {
        index: 0,
        drafts: blank_drafts(prompt),
        answering: false,
        transition: false,
        loading: prompt.is_some_and(InteractivePrompt::is_question),
    }
}
