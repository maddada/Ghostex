//! The two question surfaces: the card the agent blocks on, and the async questions a working
//! agent collects answers for without stopping.

use serde::{Deserialize, Serialize};

/// The blocking question card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionCard {
    pub visible: bool,
    /// Which question of the set the card is showing.
    pub question_index: u32,
    pub controls: QuestionControls,
    pub drafts: Vec<QuestionDraft>,
    /// The answer is in flight.
    pub answering: bool,
    /// Answering, or moving between questions: either way the controls are held.
    pub busy: bool,
    /// The saved answers are still being read back.
    pub loading: bool,
    /// An approval card's question, worded for its tool ("Allow this edit?"); empty for a question.
    #[serde(default)]
    pub approval_ask: String,
}

/// The card's primary button.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionControls {
    pub has_answer: bool,
    pub disabled: bool,
    pub label: String,
}

/// One question's saved answer: the chosen options plus free text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionDraft {
    pub indices: Vec<u32>,
    pub other: String,
}

/// The async question strip a working agent shows above the composer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsyncQuestions {
    /// The question the strip is showing, absent when nothing is pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<PendingAsyncQuestion>,
    pub draft: QuestionDraft,
    pub index: u32,
    pub count: u32,
    /// The answer text as it will be sent.
    pub answer: String,
    pub disabled: bool,
    pub can_send: bool,
    pub working: bool,
    pub collapsed: bool,
    pub submitting: bool,
    pub loading: bool,
    pub error: String,
    pub previous_disabled: bool,
    pub next_disabled: bool,
    pub selected: Vec<u32>,
    /// The key the Previous button moves to, absent on the first question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_key: Option<String>,
    /// The key the Next button moves to, absent on the last question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_key: Option<String>,
}

/// One question the async strip is waiting on, as the renderer draws it.
///
/// `options` keeps its absent-versus-empty distinction, because a question with no options is a
/// free-text answer while an empty list would draw an empty picker.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAsyncQuestion {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    /// `<message id>:<index within that message>`.
    pub key: String,
}
