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
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
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

    /// The asking call's id, when gxserver knew it.
    pub fn tool_use_id(&self) -> Option<&str> {
        match self {
            Self::Question { tool_use_id, .. } | Self::Approval { tool_use_id, .. } => {
                tool_use_id.as_deref().filter(|id| !id.is_empty())
            }
        }
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
    /// The failed queue row this card stands for (`{ promptId }`,
    /// server/src/session_chat_send_failure.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_failure: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<TerminalDialog>,
    /// The whole plan Claude's "Ready to code?" asks to approve, as Markdown from its plan file
    /// (server/src/session_chat_claude_plan.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<TerminalNoticeAction>>,
    /// Answerable picker rows, in screen order. Absent means the card only describes a state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<TerminalNoticeChoice>>,
    /// The chat cannot read this screen, so it passes keys through to it and shows `screenTail`.
    #[serde(default)]
    pub passthrough: bool,
}

impl TerminalNotice {
    /// The notice carried by `state.session.terminal_notice`, or `None`.
    pub fn parse(value: Option<&Value>) -> Option<Self> {
        serde_json::from_value(value?.clone()).ok()
    }

    /// Whether this notice owns the agent's input line, so a message sent now would land in it.
    pub fn holds_input(&self) -> bool {
        // A side question panel does not: gxserver closes it with Escape before a send lands
        // (server/src/session_chat_composer.rs).
        if self
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.side_question.is_some())
        {
            return false;
        }
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

    /// Which visible row Cmd+Enter (Ctrl+Enter) answers.
    ///
    /// CDXC:SessionChat 2026-10-01 DECISION: User: the folder-trust card's Cmd+Enter must not mean "No, exit". Claude Code lists "No, exit" above "Yes, I trust this folder", so the first row made the shortcut quit the agent; on a trust card it answers the row that grants trust (the agent's own order is kept on screen), and every other card keeps its first row.
    /// CDXC:SessionChat 2026-10-15 DECISION: User: on a card with rows, Cmd+Enter (Ctrl+Enter) presses the same thing as the labelled Confirm, the row highlighted in the terminal, never blindly the first row (on `/plugin`'s installed-plugin page the first row is "Disable plugin"). This replaces "every other card keeps its first row" above for live dialogs; the trust card's rule still wins, and cards without a live dialog keep their first row.
    pub fn primary_choice_position(&self) -> Option<usize> {
        let choices = self.visible_choices();
        if choices.is_empty() {
            return None;
        }
        if self.kind == "trustPrompt" {
            if let Some(trust) = choices.iter().position(|choice| {
                let label = choice.label.trim_start();
                label.starts_with("Yes") || label.starts_with("Trust")
            }) {
                return Some(trust);
            }
        }
        if self
            .dialog
            .as_ref()
            .is_some_and(|dialog| !dialog.rows.is_empty())
        {
            if let Some(highlighted) = choices.iter().position(|choice| choice.selected) {
                return Some(highlighted);
            }
        }
        Some(0)
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
    /// `switchToTerminal`, `sendKeys`, `recoverCodexConversation`, `trustAndRemember` or `restartAgent`.
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
    /// Claude's `/btw` panel with its answer as Markdown (server/src/session_chat_claude_panel.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_question: Option<serde_json::Value>,
    /// A Settings-style panel read as tabs, headings, tables, meters, text and code-font blocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<serde_json::Value>>,
    /// Claude's or Codex's `/resume` list read row by row (server/src/session_chat_resume_picker.rs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picker: Option<serde_json::Value>,
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
