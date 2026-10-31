//! Family c's state: the blocking question card, the async question strip, and the notices.
//!
//! **This file belongs to family c (questions, approvals, notices).** No other family edits it.
//!
//! Read from, never write to: `ChatState::session::prompt` and
//! `ChatState::session::terminal_notice` (family a folds both; an omission on a frame that can
//! carry them means CLEARED), `ChatState::session::retired_async_question_ids` and
//! `ChatState::session::async_questions_since`.

use crate::document::QuestionDraft;
use crate::questions::drafts::AnswerDrafts;
use crate::questions::notice_state::DismissedNotice;

/// What the question and notice surfaces remember between frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuestionsState {
    // ---- the blocking card ------------------------------------------------
    /// What the current prompt is, as the card identifies it (`question:<count>:<first>` or
    /// `approval:<tool>:<summary>`). `None` when nothing is blocking.
    pub prompt_key: Option<String>,
    /// The prompt key the user answered or cancelled, which hides the card until a new one
    /// arrives.
    pub dismissed_prompt: Option<String>,
    /// The asking call's `toolUseId` of the dismissed prompt, when the prompt carried one, so the
    /// same question asked again by a new call still shows.
    pub dismissed_tool_use_id: Option<String>,
    /// Which question of the set the card is showing.
    pub question_index: usize,
    /// The whole prompt's identity, which is what the saved answers are keyed by. A changed
    /// content key resets the card and re-reads its drafts.
    pub question_content_key: Option<String>,
    /// The saved answers are still being read back.
    pub question_drafts_loading: bool,
    /// A write is in flight between two questions, so the controls are held.
    pub question_transition: bool,
    /// One draft per question, in question order.
    pub question_drafts: Vec<QuestionDraft>,
    /// The answer is in flight.
    pub answering: bool,
    /// The answer call in flight, and what it dismisses when it succeeds.
    pub answer_request: Option<AnswerRequest>,
    /// The interrupt a cancelled question card asked for.
    pub cancel_request: Option<u64>,
    /// Keys and search text sent to an open picker or pass-through screen, which do not hold the
    /// card (`crate::questions::terminal_picker::is_navigation_answer`).
    pub picker_requests: Vec<u64>,
    /// The search text in flight to an open picker, and the newest text waiting behind it.
    pub picker_query_request: Option<u64>,
    pub pending_picker_query: Option<serde_json::Value>,
    /// The content key the draft write in flight belongs to; a later prompt drops its answer.
    pub draft_write_content_key: Option<String>,
    /// That write was a single-select pick, so landing it moves the card on.
    pub advance_after_write: bool,

    // ---- notices ----------------------------------------------------------
    /// `kind:detectedAt` of the notice currently on screen.
    pub active_notice_key: Option<String>,
    /// The detection whose picker the user already answered, which hides the card while gxserver
    /// still reports it.
    pub answered_notice_key: Option<String>,
    /// The detection whose answer failed, which stops blocking the composer.
    pub retired_notice_key: Option<String>,
    /// The pass-through detection the user gave the keyboard back from: its card stays hidden
    /// for as long as that detection lasts, and the composer has its keys again.
    pub released_notice_key: Option<String>,
    /// The dismissal this session remembers, read back from client storage.
    pub dismissed_notice: Option<DismissedNotice>,
    /// The refusal the notice card shows instead of the composer's error line.
    pub notice_error: Option<String>,
    /// Whether the notice card is on screen, kept true by [`crate::questions::sync`] after every
    /// event.
    ///
    /// Nothing reads it: family d calls [`crate::questions::gates::notice_visible`] directly, and
    /// the document key comes from the same gate. Kept as the cache the surfaces may move back to
    /// rather than deleted, because the gate walks the whole question list on every read.
    pub notice_visible: bool,
    /// Whether the notice on screen is the detection whose answer was refused, so it no longer
    /// holds the agent's input line. Kept beside `notice_visible`, and unread for the same reason.
    pub notice_retired: bool,

    // ---- the async strip --------------------------------------------------
    pub async_questions: AsyncQuestionsState,

    // ---- question forms ---------------------------------------------------
    /// The reader's side of each ```questions form, by form id (`crate::questions::forms`).
    pub forms: std::collections::BTreeMap<String, FormState>,

    // ---- unanswered questions --------------------------------------------
    /// The Previous question pill's place and the last jump (`crate::questions::open_questions`).
    pub open_questions: crate::questions::open_questions::OpenQuestionsState,
    /// The For later marks set here that gxserver has not confirmed yet (`crate::questions::later`).
    pub later: crate::questions::later::LaterState,
    /// The pages that ask nothing and the charts the ? pill's list names (`crate::questions::content`).
    pub content: crate::questions::content::ContentScan,
    /// The form or chart a Resources row opened in the floating modal (`crate::questions::resource_modal`).
    pub resource_modal: Option<crate::questions::resource_modal::ResourceModal>,
    /// A "Go to chat" waiting for its item to be among the loaded rows.
    pub resource_go: Option<crate::questions::resource_modal::ResourceGo>,
}

/// One question form's picks while it is open, and its send.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormState {
    /// One draft per question: the picked choices, whether "Other / none of these" is picked, and
    /// the note (or a text question's answer).
    pub drafts: Vec<QuestionDraft>,
    /// What the reader wrote in the form's own notes box (a form of two or more questions).
    pub notes: String,
    /// The saved picks have been asked for.
    pub load_requested: bool,
    /// The reader changed the form, so saved picks that arrive later no longer replace it.
    pub touched: bool,
    /// The saved picks were forgotten once the transcript carried the answers.
    pub cleared: bool,
    /// The send in flight: its call, what it drew, and the answers it carries.
    pub send: Option<FormSend>,
    /// The answers that were queued, which the form shows until the transcript carries them.
    pub sent: Option<serde_json::Value>,
    /// Why the last send was refused.
    pub error: Option<String>,
    /// The reader opened the answered form again (`formToggle`); answered forms start collapsed.
    pub expanded: bool,
}

/// A question form's answers on their way into the session's chat queue.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormSend {
    pub request_id: u64,
    pub answers: serde_json::Value,
    /// The call's parameters, sent again unchanged (same `sendRequestId`) while gxserver is away.
    pub params: serde_json::Value,
    /// See [`crate::questions::unreachable_retry`].
    pub retry: crate::questions::unreachable_retry::Retry,
}

/// The answer call in flight and what settling it means.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnswerRequest {
    pub request_id: u64,
    /// The prompt key to dismiss when this succeeds.
    pub prompt_key: Option<String>,
    /// `question`, `approval`, `terminalChoice`, `terminalDialog`, …
    pub answer_kind: String,
    /// The saved answers this send retires, for the storage clear that follows it.
    pub submitted: AnswerDrafts,
    /// The content key the clear belongs to.
    pub content_key: Option<String>,
    /// The dismissal this send replaced when it hid the card at once, put back if it fails.
    pub restore_dismissed: Option<(Option<String>, Option<String>)>,
    /// The call's parameters with its `sendRequestId`, sent again unchanged while gxserver is away.
    pub params: serde_json::Value,
    /// See [`crate::questions::unreachable_retry`].
    pub retry: crate::questions::unreachable_retry::Retry,
}

/// Async question selection, explicit submission, retirement and draft persistence.
#[derive(Clone, Debug, PartialEq)]
pub struct AsyncQuestionsState {
    /// The saved answer per question key.
    pub drafts: AnswerDrafts,
    /// The question keys this client has retired, newest last.
    pub retired: Vec<String>,
    /// Which question the strip is showing, by key. `None` follows the first pending one.
    pub active_key: Option<String>,
    pub collapsed: bool,
    pub submitting: bool,
    /// The saved answers have not been read back yet, which holds every control.
    pub loading: bool,
    /// The host is still writing pasted pictures, so an answer would be sent without them.
    pub saving_images: bool,
    /// The last send or skip refusal.
    pub error: Option<String>,
    /// The last storage refusal, which the strip shows until a write succeeds.
    pub save_error: String,
    /// The reads that have not answered yet, so `loading` drops once both have.
    pub pending_reads: u32,
    /// The one read per chat has been asked for.
    pub load_started: bool,
    /// The send or skip in flight.
    pub submit: Option<AsyncSubmit>,
}

impl Default for AsyncQuestionsState {
    fn default() -> Self {
        Self {
            drafts: AnswerDrafts::new(),
            retired: Vec::new(),
            active_key: None,
            collapsed: false,
            submitting: false,
            // The controller starts loading and only the read clears it, so the strip cannot send
            // before its saved answers are back.
            loading: true,
            saving_images: false,
            error: None,
            save_error: String::new(),
            pending_reads: 0,
            load_started: false,
            submit: None,
        }
    }
}

/// The async answer in flight.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AsyncSubmit {
    pub request_id: u64,
    /// The question key being answered.
    pub key: String,
    /// The send was a skip, which changes the refusal copy.
    pub skip: bool,
    /// The draft this send retires, so a later edit is not cleared with it.
    pub submitted: AnswerDrafts,
}
