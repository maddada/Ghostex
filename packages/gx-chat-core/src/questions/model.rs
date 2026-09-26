//! The two things the agent can block on, typed.
//!
//! Family a folds `prompt` and `terminalNotice` onto [`crate::state::SessionState`] as
//! `serde_json::Value`, because the wire keeps whatever the server sent. Family c reads them
//! through the types below, which follow `SessionChatInteractivePrompt`,
//! `SessionChatTerminalNotice` and `SessionChatTerminalDialog` in
//! `packages/shared/session-chat.ts`.
//!
//! Every optional field is `#[serde(default)]`, so a notice from a newer daemon still parses into
//! what this build understands instead of vanishing. The document keeps the original value and
//! decorates it (see `document.rs`), so a field nobody here models is still drawn.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One question a card asks.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    #[serde(default)]
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub multi_select: bool,
    /// False when the asking tool offers no free-text answer; absent for tools that always take
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_custom: Option<bool>,
    /// The tool that asked, verbatim. The server's keystroke plan dispatches on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// omp's recommended option index: its ask dialog opens with the cursor on this row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<u64>,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
}

/// One answer row of a question.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// What the agent is blocked on: a question set, or a tool approval.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum InteractivePrompt {
    Question {
        #[serde(default)]
        questions: Vec<Question>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
    Approval {
        #[serde(default)]
        tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
}

impl InteractivePrompt {
    /// The questions of a question prompt, empty for an approval.
    pub fn questions(&self) -> &[Question] {
        match self {
            Self::Question { questions, .. } => questions,
            Self::Approval { .. } => &[],
        }
    }

    /// Whether this is a question set rather than an approval.
    pub fn is_question(&self) -> bool {
        matches!(self, Self::Question { .. })
    }

    /// The prompt carried by `state.session.prompt`, or `None` when nothing is blocking.
    pub fn parse(value: Option<&Value>) -> Option<Self> {
        serde_json::from_value(value?.clone()).ok()
    }
}

/// The blocking or failed terminal state gxserver detected.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalNotice {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_tail: Option<String>,
    /// `screen` or `watchdog`. Only a screen notice re-detects continuously, which is what the
    /// dismissal cooldown keys off.
    #[serde(default)]
    pub source: String,
    /// ISO-8601 millis, and the key a client's local dismissal remembers.
    #[serde(default)]
    pub detected_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_lock: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<TerminalDialog>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<TerminalNoticeAction>>,
    /// Answerable picker rows, in screen order. Absent means the card only describes a state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<TerminalNoticeChoice>>,
}

impl TerminalNotice {
    /// The notice carried by `state.session.terminal_notice`, or `None`.
    pub fn parse(value: Option<&Value>) -> Option<Self> {
        serde_json::from_value(value?.clone()).ok()
    }

    /// Whether this notice owns the agent's input line, so a message sent now would land in it.
    pub fn holds_input(&self) -> bool {
        self.choices.as_ref().is_some_and(|rows| !rows.is_empty())
            || self.dialog.is_some()
            || self.conversation_lock.is_some()
    }

    /// The rows the card actually offers, blank labels dropped.
    pub fn visible_choices(&self) -> Vec<&TerminalNoticeChoice> {
        self.choices
            .iter()
            .flatten()
            .filter(|choice| !choice.label.trim().is_empty())
            .collect()
    }

    /// Which visible row Escape answers.
    ///
    /// CDXC:SessionChat 2026-09-26 WHY: Escape declines. A permission prompt's rows are Yes, then any "Yes, and don't ask again" / "Yes, and switch to accept edits" rows, then No, and a plan's are "Yes, and switch to bypass permissions", "Yes, manually approve edits", "Tell Claude what to change", so taking the second row made Escape grant a standing permission or start the plan. Escape answers a row that says No; only Claude's two-way choosers (resume, model or effort switch, session paused), whose second row is the alternative rather than a yes, keep it on that row, and anything else leaves Escape unanswered.
    pub fn secondary_choice_position(&self) -> Option<usize> {
        let choices = self.visible_choices();
        if let Some(no) = choices
            .iter()
            .rposition(|choice| choice.label.trim_start().starts_with("No"))
        {
            return Some(no);
        }
        let two_way = matches!(
            self.kind.as_str(),
            "resumePrompt" | "switchConfirmPrompt" | "sessionPausedPrompt"
        );
        (two_way && choices.len() > 1).then_some(1)
    }
}

/// One answerable row of a notice's picker.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalNoticeChoice {
    /// 0-based row index, which is what an answer addresses.
    #[serde(default)]
    pub index: i64,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub selected: bool,
}

/// A button a notice offers beside its picker.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalNoticeAction {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    /// `switchToTerminal`, `sendKeys` or `recoverCodexConversation`.
    #[serde(default)]
    pub kind: String,
    /// Raw bytes for `sendKeys`, written verbatim through `answerSessionChatPrompt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send: Option<String>,
}

/// A live agent-owned menu or form the chat can drive without switching to the terminal.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDialog {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub footer: String,
    #[serde(default)]
    pub rows: Vec<TerminalDialogRow>,
    /// `search`, `text`, `key`, or `null` when the dialog takes no typing.
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub input_value: String,
    #[serde(default)]
    pub actions: Vec<String>,
}

/// One row of a terminal dialog.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDialogRow {
    #[serde(default)]
    pub number: i64,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub selected: bool,
}
