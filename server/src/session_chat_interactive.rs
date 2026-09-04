use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::server::{read_runtime_text, read_session_text, session_observer_key, AppState};
use crate::session_chat::*;
use crate::session_chat_follower::{insert_optional_selected_options, insert_screen_state};
use crate::session_chat_options::cached_session_chat_screen_state;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChatQuestionOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChatQuestion {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(rename = "multiSelect")]
    pub multi_select: bool,
    /// `Some(false)` when the asking tool offers no free-text answer (Pi's
    /// `cursor_ask_question` with `allowCustom: false`). Absent for tools that
    /// always take one (Claude's "Type something" row).
    #[serde(rename = "allowCustom", skip_serializing_if = "Option::is_none")]
    pub allow_custom: Option<bool>,
    /// The tool that asked, verbatim (`AskUserQuestion`, `cursor_ask_question`,
    /// `clarify`, `ask`, …). One agent can host multiple asking tools with
    /// different terminal UIs (omp ships both its `ask` dialog and the pi
    /// cursor bridge), so the answer keystroke plan dispatches on this, not
    /// only on the agent. Absent on prompts stored before 2026-08-30.
    #[serde(rename = "toolName", skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// omp's `recommended` option index: its ask dialog opens with the cursor
    /// on this row (default 0), so arrow-key answer plans start counting here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended: Option<usize>,
    pub options: Vec<SessionChatQuestionOption>,
}

/// Rust mirror of packages/shared/session-chat.ts `SessionChatInteractivePrompt`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SessionChatInteractivePrompt {
    Question {
        questions: Vec<SessionChatQuestion>,
        /// The hook's `tool_use_id` for the call that asked, when the hook
        /// payload carried one (Claude's PreToolUse does). The post-tool event
        /// that retires the card is matched on it, so a subagent's tool traffic
        /// in the same session cannot retire a question it did not ask.
        #[serde(rename = "toolUseId", default, skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
    Approval {
        tool: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(rename = "toolUseId", default, skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
}

impl SessionChatInteractivePrompt {
    pub fn with_tool_use_id(mut self, id: Option<String>) -> Self {
        let id = id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        match &mut self {
            SessionChatInteractivePrompt::Question { tool_use_id, .. }
            | SessionChatInteractivePrompt::Approval { tool_use_id, .. } => *tool_use_id = id,
        }
        self
    }

    pub fn tool_use_id(&self) -> Option<&str> {
        match self {
            SessionChatInteractivePrompt::Question { tool_use_id, .. }
            | SessionChatInteractivePrompt::Approval { tool_use_id, .. } => tool_use_id.as_deref(),
        }
    }
}

/// Rust mirror of packages/shared/session-chat.ts `SessionChatQuestionSelection`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct SessionChatQuestionSelection {
    #[serde(default)]
    pub indices: Vec<usize>,
    #[serde(default)]
    pub other: Option<String>,
}

const APPROVAL_SUMMARY_MAX_CHARS: usize = 200;

/// Upstream `normalizeHookEventName`: camelCase → snake_case, dashes/spaces →
/// underscores, lowercased.
fn normalize_hook_event_name(value: &str) -> String {
    let mut snake = String::with_capacity(value.len() + 4);
    let mut previous_lower_or_digit = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_uppercase() && previous_lower_or_digit {
            snake.push('_');
        }
        previous_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        snake.push(ch);
    }
    let mut normalized = String::with_capacity(snake.len());
    let mut previous_was_separator = false;
    for ch in snake.chars() {
        if ch == '-' || ch.is_whitespace() {
            if !previous_was_separator {
                normalized.push('_');
            }
            previous_was_separator = true;
        } else {
            normalized.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
        }
    }
    normalized
}

pub fn is_post_tool_hook_event(event_name: Option<&str>) -> bool {
    matches!(
        normalize_hook_event_name(event_name.unwrap_or_default()).as_str(),
        "post_tool_use" | "post_tool_use_failure"
    )
}

/// Strip non-alphanumerics and lowercase — the shared spelling-insensitive
/// form the ask-tool checks below compare against.
pub fn normalize_session_chat_tool_name(tool_name: &str) -> String {
    tool_name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Upstream `isAskUserQuestionTool`: match AskUserQuestion (Claude) /
/// request_user_input (Codex 0.145) / AskQuestion (Cursor Agent) /
/// cursor_ask_question (Pi's pi-cursor-sdk bridge) / clarify (Hermes Agent) /
/// ask (oh-my-pi) spellings.
pub fn is_ask_user_question_tool(tool_name: &str) -> bool {
    matches!(
        normalize_session_chat_tool_name(tool_name).as_str(),
        "askuserquestion"
            | "askquestion"
            | "requestuserinput"
            | "cursoraskquestion"
            | "clarify"
            | "ask"
    )
}

fn truncate_approval_summary(value: &str) -> String {
    if value.chars().count() > APPROVAL_SUMMARY_MAX_CHARS {
        let mut truncated: String = value.chars().take(APPROVAL_SUMMARY_MAX_CHARS).collect();
        truncated.push('\u{2026}');
        truncated
    } else {
        value.to_string()
    }
}

/// Upstream `summarizeApprovalInput`: prefer the first present command/file_path/
/// path/url/pattern field when it is a non-empty string, else the JSON body;
/// both capped at 200 chars.
pub fn summarize_approval_input(tool_input: Option<&Value>) -> String {
    if let Some(object) = tool_input.and_then(Value::as_object) {
        let direct = ["command", "file_path", "path", "url", "pattern"]
            .iter()
            .find_map(|key| object.get(*key).filter(|value| !value.is_null()));
        if let Some(direct) = direct
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            return truncate_approval_summary(direct);
        }
    }
    let json = tool_input.map(Value::to_string).unwrap_or_default();
    truncate_approval_summary(&json)
}

/// Hermes' clarify tool hard-caps `choices` at 4 and drops the surplus before
/// the terminal panel renders, so a card offering more would offer picks the
/// terminal cannot deliver.
const HERMES_CLARIFY_MAX_CHOICES: usize = 4;

/// Upstream `parseQuestionsShape`: the canonical AskUserQuestion tool-input
/// shape (`{questions: [...]}`, also Hermes' clarify and omp's ask), plus the
/// flat single-question shape Pi's `cursor_ask_question` sends (`{question,
/// options, allowCustom}`, with `prompt`/`choices` accepted as aliases the way
/// the bridge normalizes them) and Hermes' legacy flat `{question, choices,
/// multi_select}` call. `tool_name` is the asking tool; it travels on every
/// parsed question so the answer path can pick that tool's keystroke plan.
pub fn parse_session_chat_questions(
    tool_name: Option<&str>,
    input: &Value,
) -> Option<Vec<SessionChatQuestion>> {
    let record = input.as_object()?;
    let is_cursor_ask_question = tool_name
        .is_some_and(|tool_name| normalize_session_chat_tool_name(tool_name) == "askquestion");
    let is_hermes_clarify =
        tool_name.is_some_and(|tool_name| normalize_session_chat_tool_name(tool_name) == "clarify");
    let raw_questions: Vec<&Value> = match record.get("questions").and_then(Value::as_array) {
        Some(array) if !array.is_empty() => array.iter().collect(),
        // No questions array → try the input object itself as one question.
        _ => vec![input],
    };
    let mut questions = Vec::new();
    let bare_entry_record = Map::new();
    for raw in raw_questions {
        // Hermes tolerates bare-string batch entries (["Q1?", "Q2?"]).
        let bare_question = raw.as_str().map(str::trim).filter(|text| !text.is_empty());
        let record = match raw.as_object() {
            Some(record) => record,
            None if bare_question.is_some() => &bare_entry_record,
            None => continue,
        };
        let text = bare_question
            .map(str::to_string)
            .or_else(|| {
                record
                    .get("question")
                    .or_else(|| record.get("prompt"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        let mut options = parse_session_chat_question_options(
            record.get("options").or_else(|| record.get("choices")),
        );
        if is_hermes_clarify {
            options.truncate(HERMES_CLARIFY_MAX_CHOICES);
        }
        if !text.is_empty() || !options.is_empty() {
            // The spec uses strict === true; anything else is single-select.
            // `multi_select` is Hermes' spelling, `multi` is omp's; Hermes
            // additionally honors it only when choices exist.
            let multi_select = is_cursor_ask_question
                || record
                    .get("multiSelect")
                    .or_else(|| record.get("multi_select"))
                    .or_else(|| record.get("multi"))
                    .and_then(Value::as_bool)
                    == Some(true)
                    && !(is_hermes_clarify && options.is_empty());
            questions.push(SessionChatQuestion {
                question: text,
                header: record
                    .get("header")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                multi_select,
                allow_custom: record.get("allowCustom").and_then(Value::as_bool),
                tool_name: tool_name.map(str::to_string),
                recommended: record
                    .get("recommended")
                    .and_then(Value::as_u64)
                    .map(|index| index as usize),
                options,
            });
        }
    }
    (!questions.is_empty()).then_some(questions)
}

fn parse_session_chat_question_options(raw: Option<&Value>) -> Vec<SessionChatQuestionOption> {
    let Some(array) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|option| match option {
            Value::String(label) => Some(SessionChatQuestionOption {
                label: label.clone(),
                description: None,
            }),
            // Pi options carry `{label, value}`; the label is what its select
            // renders, with `value` as the fallback the bridge also uses.
            Value::Object(record) => Some(SessionChatQuestionOption {
                label: record
                    .get("label")
                    .or_else(|| record.get("value"))
                    .and_then(Value::as_str)?
                    .to_string(),
                description: record
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }),
            _ => None,
        })
        .collect()
}

/*
Cursor Agent does not currently write a pending AskQuestion tool call to its
JSONL transcript or forward its input through the hook event. The live panel is
therefore the only source of truth while the agent is blocked. Recognize the
panel's exact chrome and only when it still owns the bottom of the screen; an
answered panel left in scrollback must never become a stale chat card.
*/
pub fn detect_cursor_question_prompt(
    agent: Option<&str>,
    screen_text: &str,
) -> Option<SessionChatInteractivePrompt> {
    if !matches!(agent.map(str::trim), Some("cursor" | "cursor-agent")) {
        return None;
    }

    let lines: Vec<String> = screen_text
        .lines()
        .map(|line| {
            let normalized = crate::session_chat_options::normalize_spaces(
                &crate::session_chat_options::strip_ansi_sgr(line),
            );
            let mut line = normalized.trim();
            if line.starts_with('│') && line.ends_with('│') {
                line = line
                    .strip_prefix('│')
                    .and_then(|line| line.strip_suffix('│'))
                    .unwrap_or(line)
                    .trim();
            }
            line.to_string()
        })
        .collect();
    let question_marker = lines.iter().rposition(|line| {
        let Some(rest) = line.strip_prefix("Question ") else {
            return false;
        };
        let Some((current, total)) = rest.split_once(" of ") else {
            return false;
        };
        current.parse::<usize>().is_ok() && total.parse::<usize>().is_ok()
    })?;
    let footer = lines
        .iter()
        .enumerate()
        .skip(question_marker + 1)
        .find(|(_, line)| {
            line.contains("Space select")
                && line.contains("Enter next/submit")
                && line.contains("Esc to skip")
        })
        .map(|(index, _)| index)?;
    if lines.iter().skip(footer + 1).any(|line| {
        !line.is_empty()
            && !line.starts_with('└')
            && !line.starts_with('┌')
            && !line.chars().all(|ch| matches!(ch, '─' | '┘' | '┐' | ' '))
    }) {
        return None;
    }

    let (question_index, question) = lines
        .iter()
        .enumerate()
        .skip(question_marker + 1)
        .take(footer.saturating_sub(question_marker + 1))
        .find_map(|(index, line)| {
            let (number, text) = line.split_once('.')?;
            (number.trim().parse::<usize>().is_ok() && !text.trim().is_empty())
                .then(|| (index, text.trim().to_string()))
        })?;
    let mut options = Vec::new();
    let mut allow_custom = false;
    for line in lines.iter().take(footer).skip(question_index + 1) {
        let Some((_, after_checkbox)) = line.split_once("[ ]") else {
            continue;
        };
        let label = after_checkbox.trim();
        if label.starts_with("Other:") {
            allow_custom = true;
        } else if !label.is_empty() {
            options.push(SessionChatQuestionOption {
                label: label.to_string(),
                description: None,
            });
        }
    }
    if options.is_empty() && !allow_custom {
        return None;
    }
    let header = lines[..question_marker]
        .iter()
        .rev()
        .find(|line| {
            !line.is_empty()
                && !line.starts_with('┌')
                && !line.chars().all(|ch| matches!(ch, '─' | '┐' | ' '))
        })
        .cloned();
    Some(SessionChatInteractivePrompt::Question {
        questions: vec![SessionChatQuestion {
            question,
            header,
            multi_select: true,
            allow_custom: Some(allow_custom),
            tool_name: Some("AskQuestion".to_string()),
            recommended: None,
            options,
        }],
        tool_use_id: None,
    })
}

/*
CDXC:SessionChat 2026-07-31:
Hook-side prompt derivation (upstream `deriveInteractivePrompt`): an
AskUserQuestion-ish tool with input on a NON-post-tool event becomes a
question card; a `PermissionRequest` event with a tool name becomes an
approval card. Everything else derives nothing. The derived wire shape (not
the raw tool input) is what gxserver stores under
runtimeSettings.agentActivity.sessionChatPrompt, so read/stream paths never
re-parse tool payloads.
*/
pub fn derive_session_chat_prompt(
    tool_name: Option<&str>,
    tool_input: Option<&Value>,
    event_name: Option<&str>,
) -> Option<SessionChatInteractivePrompt> {
    let tool_name = tool_name.map(str::trim).filter(|value| !value.is_empty())?;
    if is_ask_user_question_tool(tool_name)
        && !is_post_tool_hook_event(event_name)
        && tool_input.is_some_and(|value| !value.is_null())
    {
        let questions = parse_session_chat_questions(Some(tool_name), tool_input?)?;
        return Some(SessionChatInteractivePrompt::Question {
            questions,
            tool_use_id: None,
        });
    }
    if event_name == Some("PermissionRequest") {
        let summary = summarize_approval_input(tool_input);
        return Some(SessionChatInteractivePrompt::Approval {
            tool: tool_name.to_string(),
            summary: (!summary.is_empty()).then_some(summary),
            tool_use_id: None,
        });
    }
    None
}

/// Post-tool events and Stop/SessionEnd/idle transitions clear a pending
/// prompt; other events leave it alone (the contract's clear rule — narrower
/// than the upstream overwrite-on-every-event rule, so unrelated working events cannot
/// drop a still-pending card).
///
/// This is the tool-blind rule: it answers for hook scripts that forward no
/// tool identity. Hook ingest uses `session_chat_prompt_clear_decision`, which
/// scopes post-tool events to the call that asked.
pub fn should_clear_session_chat_prompt(
    event_name: Option<&str>,
    next_activity: Option<&str>,
) -> bool {
    if is_post_tool_hook_event(event_name) {
        return true;
    }
    if next_activity == Some("idle") {
        return true;
    }
    matches!(
        normalize_hook_event_name(event_name.unwrap_or_default()).as_str(),
        "stop" | "session_end" | "idle"
    )
}

/// One hook event's view of a stored prompt, for `session_chat_prompt_clear_decision`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SessionChatPromptClearEvent<'a> {
    pub event_name: Option<&'a str>,
    pub next_activity: Option<&'a str>,
    pub tool_name: Option<&'a str>,
    pub tool_use_id: Option<&'a str>,
    /// Claude's 60-second "waiting for your input" reminder
    /// (`notificationKind: idleInput`), which settles ACTIVITY to idle.
    pub idle_input_notification: bool,
    /// The event's `tool_input`, which scopes a post-tool event to an approval
    /// card when neither side carries a `tool_use_id`.
    pub tool_input: Option<&'a Value>,
}

/*
CDXC:AgentScreenDetection 2026-09-03:
Whether a hook event retires the stored card, given the card it would retire.

Claude Code runs the PreToolUse/PostToolUse hooks of every background subagent
under the LEAD session's id, so while the lead sits on an AskUserQuestion the
session keeps receiving post-tool events for Bash/Read/Grep calls the subagents
make. The tool-blind rule above treated each of those as "the tool finished"
and dropped the card within a second of the PreToolUse hook storing it — and
because Claude does not flush a pending tool call to its transcript, nothing
could bring the card back. Observed 2026-09-03: two multi-select questions
visible in the terminal, neither ever shown in Session Chat, both with a
background agent working the same session.

A post-tool event therefore retires the card only when it is the asking call's
own completion: same `tool_use_id` when both sides carry one, otherwise the
same tool (an ask-tool for a question card, the approved tool for an approval
card). A post-tool event with no tool identity at all keeps the old behaviour,
because a hook script that forwards nothing cannot be scoped.

CDXC:AgentScreenDetection 2026-09-04 WHY:
Claude's `PermissionRequest` hook payload carries no `tool_use_id` at all
(`tool_name`, `tool_input`, `permission_suggestions` only), so every approval
card falls to the same-tool rule, and a parallel batch of Bash calls retired
the card the moment a SIBLING call finished while the dialog was still on
screen (observed 2026-09-04, a cd-compound-read prompt with no card in chat).
The approved call's input is on both sides, so an approval card with a summary
is retired only by a post-tool event whose `tool_input` summarizes to that same
text; another input is another call and keeps the card.

Claude's idle-input reminder is a statement about the input line, not about
the question: the question IS what the input is waiting on. It moves activity
to idle and must not take the card with it; a cancelled question retires
through its transcript tool result instead.
*/
pub fn session_chat_prompt_clear_decision(
    stored: Option<&SessionChatInteractivePrompt>,
    event: SessionChatPromptClearEvent<'_>,
) -> bool {
    let Some(stored) = stored else {
        return false;
    };
    if is_post_tool_hook_event(event.event_name) {
        return post_tool_event_resolves_prompt(
            stored,
            event.tool_name,
            event.tool_use_id,
            event.tool_input,
        );
    }
    if event.idle_input_notification {
        return false;
    }
    if event.next_activity == Some("idle") {
        return true;
    }
    matches!(
        normalize_hook_event_name(event.event_name.unwrap_or_default()).as_str(),
        "stop" | "session_end" | "idle"
    )
}

/// Whether a post-tool event for `tool_name`/`tool_use_id` is the completion
/// of the call that produced `stored`.
pub fn post_tool_event_resolves_prompt(
    stored: &SessionChatInteractivePrompt,
    tool_name: Option<&str>,
    tool_use_id: Option<&str>,
    tool_input: Option<&Value>,
) -> bool {
    let tool_use_id = tool_use_id.map(str::trim).filter(|value| !value.is_empty());
    if let (Some(event_id), Some(stored_id)) = (tool_use_id, stored.tool_use_id()) {
        return event_id == stored_id;
    }
    let Some(tool_name) = tool_name.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    match stored {
        SessionChatInteractivePrompt::Question { .. } => is_ask_user_question_tool(tool_name),
        SessionChatInteractivePrompt::Approval { tool, summary, .. } => {
            if normalize_session_chat_tool_name(tool) != normalize_session_chat_tool_name(tool_name)
            {
                return false;
            }
            match (summary, tool_input.filter(|value| !value.is_null())) {
                (Some(summary), Some(tool_input)) => {
                    summarize_approval_input(Some(tool_input)) == *summary
                }
                _ => true,
            }
        }
    }
}

pub fn parse_stored_session_chat_prompt(stored: &str) -> Option<SessionChatInteractivePrompt> {
    serde_json::from_str::<SessionChatInteractivePrompt>(stored).ok()
}

/*
CDXC:SessionChat 2026-08-01:
Transcript-derived question detection. Hook delivery of `toolName`/`toolInput`
is optional (older installed hook scripts do not forward it) and PostToolUse is
not guaranteed, so the transcript is the second, independent source of truth for
the question card: an AskUserQuestion/request_user_input tool call with no tool
result after it is still waiting for an answer, and a tool result after it means
the user already answered — in the chat card or straight in the terminal.

The fold is incremental so the follower can advance it over appended batches
without buffering messages, and restarts from a snapshot's tail window.
*/
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionChatTranscriptPromptState {
    pending: Option<SessionChatInteractivePrompt>,
    last_question: Option<SessionChatInteractivePrompt>,
    answered: bool,
}

impl SessionChatTranscriptPromptState {
    pub fn advance(&mut self, messages: &[SessionChatMessage]) {
        for message in messages {
            let question_call = message.blocks.iter().any(|block| {
                matches!(
                    block,
                    SessionChatBlock::ToolCall { name, .. } if is_ask_user_question_tool(name)
                )
            });
            let answer_after_question = self.pending.is_some()
                && message.role == SessionChatRole::Assistant
                && !question_call
                && message
                    .blocks
                    .iter()
                    .any(|block| matches!(block, SessionChatBlock::Text { .. }));
            if answer_after_question {
                self.answered = true;
                self.pending = None;
            }
            for block in &message.blocks {
                match block {
                    SessionChatBlock::ToolCall { name, input }
                        if is_ask_user_question_tool(name) =>
                    {
                        self.answered = false;
                        self.pending =
                            parse_session_chat_questions(Some(name), input).map(|questions| {
                                SessionChatInteractivePrompt::Question {
                                    questions,
                                    tool_use_id: None,
                                }
                            });
                        self.last_question = self.pending.clone();
                    }
                    SessionChatBlock::ToolResult { .. } => {
                        if self.last_question.is_some() {
                            self.answered = true;
                        }
                        self.pending = None;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn pending(&self) -> Option<&SessionChatInteractivePrompt> {
        self.pending.as_ref()
    }

    /// True once the most recent AskUserQuestion call has its tool result.
    pub fn answered(&self) -> bool {
        self.last_question.is_some() && self.answered
    }

    /// The most recent question the transcript proves was answered. `None`
    /// while the latest question is still pending (or was unparsable).
    pub fn answered_question(&self) -> Option<&SessionChatInteractivePrompt> {
        if self.answered {
            self.last_question.as_ref()
        } else {
            None
        }
    }
}

pub fn scan_transcript_prompt_state(
    messages: &[SessionChatMessage],
) -> SessionChatTranscriptPromptState {
    let mut state = SessionChatTranscriptPromptState::default();
    state.advance(messages);
    state
}

/// Question texts of a question prompt, in order. `None` for approvals.
fn session_chat_prompt_question_texts(prompt: &SessionChatInteractivePrompt) -> Option<Vec<&str>> {
    match prompt {
        SessionChatInteractivePrompt::Question { questions, .. } => Some(
            questions
                .iter()
                .map(|question| question.question.as_str())
                .collect(),
        ),
        SessionChatInteractivePrompt::Approval { .. } => None,
    }
}

/// Hook-derived prompts stay authoritative — they carry approvals and richer
/// payloads the transcript cannot express. The transcript only adds a card the
/// hooks never reported, or retires a question card the transcript proves was
/// answered.
///
/// Retirement is matched by question text: an answered question retires the
/// stored card only when it asks the same questions. Claude Code does not
/// flush the assistant row for a *pending* AskUserQuestion, so while a new
/// question waits, the transcript's most recent question is the previous
/// (answered) one — an unconditional `answered()` check retired every stored
/// question after the first one in the tail window, hiding the card from all
/// report sites.
pub fn resolve_session_chat_prompt(
    stored: Option<SessionChatInteractivePrompt>,
    transcript: &SessionChatTranscriptPromptState,
) -> Option<SessionChatInteractivePrompt> {
    match stored {
        Some(prompt) => {
            let retired = transcript.answered_question().is_some_and(|answered| {
                match (
                    session_chat_prompt_question_texts(answered),
                    session_chat_prompt_question_texts(&prompt),
                ) {
                    (Some(answered_texts), Some(stored_texts)) => answered_texts == stored_texts,
                    _ => false,
                }
            });
            if retired {
                None
            } else {
                Some(prompt)
            }
        }
        None => transcript.pending().cloned(),
    }
}

/// Builds a `sessionChatState` frame carrying a prompt change (hook ingest) or
/// a fresh model/effort detection (post-dispatch probe) so a producer outside
/// the follower task can push it through a live stream.
///
/// CDXC:AgentScreenDetection 2026-08-19: `terminal_notice` is the notice
/// the session is showing right now, not a delta — an omitted field clears the
/// client's card, so every producer restates it.
#[allow(clippy::too_many_arguments)]
pub fn build_session_chat_prompt_state_frame(
    project_id: &str,
    session_id: &str,
    epoch: i64,
    seq: i64,
    status: SessionChatStatus,
    prompt: Option<&SessionChatInteractivePrompt>,
    agent_session_id: Option<&str>,
    protocol_version: u64,
    server_id: &str,
    working: bool,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
    screen: SessionChatScreenState<'_>,
    // CDXC:SessionChat 2026-08-21: the session's queue + draft.
    // Every producer of a state frame restates them, because a client replaces
    // its list with whatever a state frame carries; omitting them here is how a
    // pre-queue daemon is recognised, not how "unchanged" is expressed.
    queue: Option<&crate::session_chat_queue::SessionChatQueueSnapshot>,
) -> Value {
    let mut frame = Map::new();
    frame.insert("type".to_string(), json!("sessionChatState"));
    frame.insert("projectId".to_string(), json!(project_id));
    frame.insert("sessionId".to_string(), json!(session_id));
    frame.insert("epoch".to_string(), json!(epoch));
    frame.insert("seq".to_string(), json!(seq));
    frame.insert("protocolVersion".to_string(), json!(protocol_version));
    frame.insert("serverId".to_string(), json!(server_id));
    frame.insert("status".to_string(), json!(status.as_str()));
    frame.insert("working".to_string(), json!(working));
    if let Some(prompt) = prompt.or(screen.prompt) {
        if let Ok(value) = serde_json::to_value(prompt) {
            frame.insert("prompt".to_string(), value);
        }
    }
    insert_optional_selected_options(&mut frame, selected_options);
    insert_screen_state(&mut frame, screen);
    crate::session_chat_returned_prompt::insert_session_chat_returned_prompt(
        &mut frame, project_id, session_id,
    );
    if let Some(queue) = queue {
        queue.insert_into(&mut frame);
    }
    if let Some(agent_session_id) = agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        frame.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    Value::Object(frame)
}

// ---------------------------------------------------------------------------
// Inline sanity tests (real transcript files are skipped when absent)
// ---------------------------------------------------------------------------

/*
CDXC:SessionChat 2026-07-31:
Prompt changes ride the LIVE follower stream: hook ingest reports
sessionChatPromptChanged, and this emits a sessionChatState frame through the
session's SessionChatStream (same epoch, next seq) so subscribed clients
show/clear the interactive card without waiting for a transcript drain. No
follower/no subscribers → nothing to emit; the prompt still reaches clients
via readSessionChat and the next authoritative snapshot.
*/
pub(crate) fn emit_session_chat_prompt_state_frame(state: &AppState, session: &Value) {
    let (Some(project_id), Some(session_id)) = (
        read_session_text(session, "projectId"),
        read_session_text(session, "sessionId"),
    ) else {
        return;
    };
    let stream = {
        let Ok(followers) = state.session_chat_followers.lock() else {
            return;
        };
        let Some(entry) = followers.get(&session_observer_key(&project_id, &session_id)) else {
            return;
        };
        let follower_active =
            entry.subscribers > 0 && entry.task.as_ref().is_some_and(|task| !task.is_finished());
        if !follower_active {
            return;
        }
        entry.stream.clone()
    };
    let prompt = crate::agents::session_chat_prompt_setting(session)
        .as_deref()
        .and_then(crate::session_chat::parse_stored_session_chat_prompt);
    let working = session_chat_hook_working(session);
    let status = if working {
        crate::session_chat::SessionChatStatus::Working
    } else {
        crate::session_chat::SessionChatStatus::Ready
    };
    let agent_session_id = read_runtime_text(session, "agentSessionId");
    /*
    CDXC:AgentScreenDetection 2026-08-19:
    Clients treat an OMITTED `terminalNotice` on a state frame as "cleared"
    (prompt semantics), so a prompt-driven frame has to re-state the notice the
    session is currently showing or the card would blink away on every hook
    event. Cached read only — this runs on the hook-ingest thread.
    */
    let screen = cached_session_chat_screen_state(state, &project_id, &session_id);
    /*
    CDXC:SessionChat 2026-08-21: a state frame REPLACES the
    client's queue, so every producer of one restates the current rows rather
    than leaving them out.
    */
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot(
        &state.paths,
        &project_id,
        &session_id,
    );
    /*
    The seq must be taken and the frame published as one step: this runs on a
    hook-ingest thread while the follower task publishes into the SAME counter,
    and a frame that reaches the hub out of seq order makes every client treat
    it as a gap and force a resync.

    CDXC:AgentScreenDetection 2026-08-24: the EPOCH is read in here for
    the same reason. Sampled before the call, the follower could start a new
    generation in between — the frame then carried a new-generation seq stamped
    with the retired epoch, which clients read as a gap and answered with a
    spurious resync. `begin_generation` takes this same lock, so reading epoch
    and seq under it makes the pair coherent.
    */
    stream.emit_sequenced(
        |seq| {
            let (epoch, _) = stream.current();
            crate::session_chat::build_session_chat_prompt_state_frame(
                &project_id,
                &session_id,
                epoch,
                seq,
                status,
                prompt.as_ref(),
                agent_session_id.as_deref(),
                GXSERVER_PROTOCOL_VERSION,
                &state.metadata.server_id,
                working,
                None,
                screen.borrow(),
                Some(&queue),
            )
        },
        |frame| state.event_hub.broadcast(frame),
    );
}
