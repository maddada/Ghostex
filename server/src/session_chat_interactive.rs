use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::server::{read_runtime_text, read_session_text, session_observer_key, AppState};
use crate::session_chat::*;
use crate::session_chat_follower::{insert_optional_selected_options, insert_screen_state};
use crate::session_chat_options::cached_session_chat_screen_state;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionChatQuestionOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionChatQuestion {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(rename = "multiSelect")]
    pub multi_select: bool,
    pub options: Vec<SessionChatQuestionOption>,
}

/// Rust mirror of packages/shared/session-chat.ts `SessionChatInteractivePrompt`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SessionChatInteractivePrompt {
    Question {
        questions: Vec<SessionChatQuestion>,
    },
    Approval {
        tool: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
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

/// Upstream `isAskUserQuestionTool`: strip non-alphanumerics, lowercase, and match
/// AskUserQuestion (Claude) / request_user_input (Codex 0.145) spellings.
pub fn is_ask_user_question_tool(tool_name: &str) -> bool {
    let normalized: String = tool_name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    normalized == "askuserquestion" || normalized == "requestuserinput"
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

/// Upstream `parseQuestionsShape`: the canonical AskUserQuestion tool-input shape.
pub fn parse_session_chat_questions(input: &Value) -> Option<Vec<SessionChatQuestion>> {
    let raw_questions = input.as_object()?.get("questions")?.as_array()?;
    if raw_questions.is_empty() {
        return None;
    }
    let mut questions = Vec::new();
    for raw in raw_questions {
        let Some(record) = raw.as_object() else {
            continue;
        };
        let text = record
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let options = parse_session_chat_question_options(record.get("options"));
        if !text.is_empty() || !options.is_empty() {
            questions.push(SessionChatQuestion {
                question: text,
                header: record
                    .get("header")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                // The spec uses strict === true; anything else is single-select.
                multi_select: record.get("multiSelect").and_then(Value::as_bool) == Some(true),
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
            Value::Object(record) => Some(SessionChatQuestionOption {
                label: record.get("label").and_then(Value::as_str)?.to_string(),
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
CDXC:SessionChatSend 2026-07-31:
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
        let questions = parse_session_chat_questions(tool_input?)?;
        return Some(SessionChatInteractivePrompt::Question { questions });
    }
    if event_name == Some("PermissionRequest") {
        let summary = summarize_approval_input(tool_input);
        return Some(SessionChatInteractivePrompt::Approval {
            tool: tool_name.to_string(),
            summary: (!summary.is_empty()).then_some(summary),
        });
    }
    None
}

/// Post-tool events and Stop/SessionEnd/idle transitions clear a pending
/// prompt; other events leave it alone (the contract's clear rule — narrower
/// than the upstream overwrite-on-every-event rule, so unrelated working events cannot
/// drop a still-pending card).
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

pub fn parse_stored_session_chat_prompt(stored: &str) -> Option<SessionChatInteractivePrompt> {
    serde_json::from_str::<SessionChatInteractivePrompt>(stored).ok()
}

/*
CDXC:SessionChatCore 2026-08-01:
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
            for block in &message.blocks {
                match block {
                    SessionChatBlock::ToolCall { name, input }
                        if is_ask_user_question_tool(name) =>
                    {
                        self.answered = false;
                        self.pending = parse_session_chat_questions(input)
                            .map(|questions| SessionChatInteractivePrompt::Question { questions });
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
        SessionChatInteractivePrompt::Question { questions } => Some(
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
/// CDXC:SessionChatTerminalNotices 2026-08-19: `terminal_notice` is the notice
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
    // CDXC:SessionChatQueueCarriage 2026-08-21: the session's queue + draft.
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
    if let Some(prompt) = prompt {
        if let Ok(value) = serde_json::to_value(prompt) {
            frame.insert("prompt".to_string(), value);
        }
    }
    insert_optional_selected_options(&mut frame, selected_options);
    insert_screen_state(&mut frame, screen);
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
CDXC:SessionChatSend 2026-07-31:
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
    CDXC:SessionChatTerminalNotices 2026-08-19:
    Clients treat an OMITTED `terminalNotice` on a state frame as "cleared"
    (prompt semantics), so a prompt-driven frame has to re-state the notice the
    session is currently showing or the card would blink away on every hook
    event. Cached read only — this runs on the hook-ingest thread.
    */
    let screen = cached_session_chat_screen_state(state, &project_id, &session_id);
    /*
    CDXC:SessionChatQueueCarriage 2026-08-21: a state frame REPLACES the
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

    CDXC:SessionChatFollowerLiveness 2026-08-24: the EPOCH is read in here for
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
