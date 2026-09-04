/*
CDXC:SessionChat 2026-09-04 DECISION:
User: when a chat-box Escape lands right after a send and Claude Code hands the
prompt back to its own composer, the chat view must reflect that instead of
showing the message as sent: the user bubble goes away, the text comes back
into the chat composer (above anything typed since), the session stops
pretending to work, and an "Interrupted the agent" row says what happened.
Claude Code only; Codex really sends, and every other agent is measured later.

CDXC:SessionChat 2026-09-04 WHY:
Measured live on Claude Code 2.1.260 through a zmx pty. An Escape ~85ms after
Enter leaves NO user row in the JSONL at all; an Escape a few hundred
milliseconds later (any time before the first assistant chunk) leaves the user
row with no interrupt marker of any kind, and the next prompt's parentUuid
skips it. In both cases Claude restores the exact text to its input box, its
history area goes blank, and it fires no Stop hook, so the session sat at
`working` forever and a fresh session's chat stayed on "Loading conversation…".
"Not in the transcript" is therefore not the signal. The signal is the screen:
the composer is back and holds the text that was just sent, with nothing
recorded after it. Detection runs only for a send this daemon made from Chat
(the last-send registry below), so Escape on a terminal-typed prompt is left
alone until the screen-probe trigger exists.

The terminal composer is cleared with the same Ctrl+U/Ctrl+K burst a chat send
uses rather than the prompt-editor draft handshake: the text is already known
here, so there is nothing to transfer, the burst is immediate where the
handshake can hold for 16s, and it leaves no Saved Prompts residue.

SEE-ALSO: session_chat_send.rs (interrupt endpoint, last-send registry calls),
session_chat_queue_runtime.rs (registry, `sendCancelled`), session_chat_follower.rs
and session_chat_read.rs (row filter, `returnedPrompt` carriage),
packages/core-ui/chat/session-chat-returned-prompt.ts (client apply-once).
*/

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::{
    agents::identity::normalize_agent_id,
    logging::{GxserverLogInput, LogLevel},
    server::AppState,
    session_chat::{
        resolve_session_chat_transcript_agent, SessionChatBlock, SessionChatMessage,
        SessionChatRole, SessionChatSource, SessionChatTailPage, SessionChatTranscriptAgent,
        SessionChatTurnLifecycle, SessionChatTurnLifecycleState,
    },
    session_chat_composer::{claude_composer_input_text, session_chat_composer_agent_id},
    session_chat_follower::session_chat_agent_for_session,
    session_chat_send::{
        build_agent_tui_clear_input_steps, capture_session_terminal_text,
        enqueue_session_chat_send, SessionChatSendTarget,
    },
};

/// How long after Escape the screen is polled for the returned prompt. Claude
/// repaints the composer within a few hundred milliseconds; the ceiling only
/// bounds a screen that never shows it (a real mid-response interrupt).
const RETURNED_PROMPT_DETECT_SETTLE: Duration = Duration::from_millis(400);
const RETURNED_PROMPT_DETECT_POLL: Duration = Duration::from_millis(250);
const RETURNED_PROMPT_DETECT_TIMEOUT: Duration = Duration::from_millis(3_500);
/// A chat send older than this is not what an Escape is answering.
const LAST_CHAT_SEND_MAX_AGE: Duration = Duration::from_secs(30 * 60);
/// How long `returnedPrompt` rides frames and reads so a client can put the
/// text back into its composer. A client applies each id once.
const RETURNED_PROMPT_COMPOSER_TTL: Duration = Duration::from_secs(120);
/// How long the abandoned transcript row stays filtered. The next prompt
/// abandons it for good through the parent-skip rule, so this only has to
/// cover the wait until then.
const RETURNED_PROMPT_RETRACTION_TTL: Duration = Duration::from_secs(12 * 60 * 60);
/// Normalized characters of the sent text the composer must start with.
const RETURNED_PROMPT_MATCH_CHARS: usize = 24;
/// Claude's collapsed large-paste placeholder, normalized like the screen.
const RETURNED_PROMPT_PASTED_PLACEHOLDER: &str = "[pastedtext";
/// A user row older than this relative to the send is an earlier prompt.
const RETURNED_PROMPT_ROW_SLACK_MS: i64 = 5_000;
/// Transcript rows read for the shape check.
const RETURNED_PROMPT_TAIL_LIMIT: usize = 16;

fn store_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

// ---------------------------------------------------------------------------
// Last chat send registry
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct LastChatSend {
    text: String,
    image_paths: Vec<String>,
    started_at: Instant,
    sent_at_ms: i64,
    /// Enter was written. False while the worker is still typing, or when the
    /// interrupt cancelled the send before Enter.
    submitted: bool,
}

type LastSendStore = Mutex<HashMap<String, LastChatSend>>;

fn last_sends() -> &'static LastSendStore {
    static STORE: OnceLock<LastSendStore> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Called by the chat send path right before the send is executed.
pub fn record_session_chat_send_started(
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
) {
    if let Ok(mut sends) = last_sends().lock() {
        sends.insert(
            store_key(project_id, session_id),
            LastChatSend {
                text: text.to_string(),
                image_paths: image_paths.to_vec(),
                started_at: Instant::now(),
                sent_at_ms: chrono::Utc::now().timestamp_millis(),
                submitted: false,
            },
        );
    }
}

/// Called once the send's Enter reached zmx.
pub fn record_session_chat_send_submitted(project_id: &str, session_id: &str) {
    if let Ok(mut sends) = last_sends().lock() {
        if let Some(send) = sends.get_mut(&store_key(project_id, session_id)) {
            send.submitted = true;
        }
    }
}

fn take_last_chat_send(project_id: &str, session_id: &str) -> Option<LastChatSend> {
    let mut sends = last_sends().lock().ok()?;
    sends.remove(&store_key(project_id, session_id))
}

// ---------------------------------------------------------------------------
// Returned prompt store
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SessionChatReturnedPrompt {
    pub id: String,
    pub text: String,
    pub image_paths: Vec<String>,
    /// The transcript row Claude wrote for the prompt, when it wrote one.
    pub message_id: Option<String>,
    /// RFC3339 millis.
    pub at: String,
    stored_at: Instant,
    /// The row id still has to be retracted through a live frame.
    retraction_pending: bool,
}

impl SessionChatReturnedPrompt {
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("id".to_string(), json!(self.id));
        map.insert("text".to_string(), json!(self.text));
        if !self.image_paths.is_empty() {
            map.insert("imagePaths".to_string(), json!(self.image_paths));
        }
        map.insert("at".to_string(), json!(self.at));
        Value::Object(map)
    }
}

type ReturnedPromptStore = Mutex<HashMap<String, SessionChatReturnedPrompt>>;

fn store() -> &'static ReturnedPromptStore {
    static STORE: OnceLock<ReturnedPromptStore> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn current(project_id: &str, session_id: &str) -> Option<SessionChatReturnedPrompt> {
    let mut entries = store().lock().ok()?;
    let key = store_key(project_id, session_id);
    let entry = entries.get(&key)?;
    if entry.stored_at.elapsed() >= RETURNED_PROMPT_RETRACTION_TTL {
        entries.remove(&key);
        return None;
    }
    Some(entry.clone())
}

/// The prompt a client should put back into its composer, while it is fresh.
pub fn session_chat_returned_prompt(
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatReturnedPrompt> {
    current(project_id, session_id)
        .filter(|entry| entry.stored_at.elapsed() < RETURNED_PROMPT_COMPOSER_TTL)
}

pub fn insert_session_chat_returned_prompt(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    if let Some(entry) = session_chat_returned_prompt(project_id, session_id) {
        frame.insert("returnedPrompt".to_string(), entry.to_value());
    }
}

/// Fingerprint term for the long-poll read loop: the id only.
pub fn session_chat_returned_prompt_identity(project_id: &str, session_id: &str) -> String {
    session_chat_returned_prompt(project_id, session_id)
        .map(|entry| entry.id)
        .unwrap_or_default()
}

/// Removes the returned prompt's transcript row from a page or window and
/// turns the lifecycle that row started into an interrupted one.
pub fn filter_session_chat_returned_prompts(
    project_id: &str,
    session_id: &str,
    messages: &mut Vec<SessionChatMessage>,
    lifecycle: &mut Option<SessionChatTurnLifecycle>,
) {
    let Some(entry) = current(project_id, session_id) else {
        return;
    };
    let Some(message_id) = entry.message_id.as_deref() else {
        return;
    };
    messages.retain(|message| message.id != message_id);
    if lifecycle
        .as_ref()
        .is_some_and(|current| current.turn_id == message_id)
    {
        *lifecycle = Some(interrupted_lifecycle(&entry));
    }
}

/// The retraction a follower still owes its clients, once.
pub fn take_session_chat_returned_prompt_retraction(
    project_id: &str,
    session_id: &str,
) -> Option<(Vec<String>, SessionChatTurnLifecycle)> {
    let mut entries = store().lock().ok()?;
    let entry = entries.get_mut(&store_key(project_id, session_id))?;
    if !entry.retraction_pending {
        return None;
    }
    let message_id = entry.message_id.clone()?;
    entry.retraction_pending = false;
    let lifecycle = interrupted_lifecycle(entry);
    Some((vec![message_id], lifecycle))
}

fn interrupted_lifecycle(entry: &SessionChatReturnedPrompt) -> SessionChatTurnLifecycle {
    SessionChatTurnLifecycle {
        state: SessionChatTurnLifecycleState::Interrupted,
        turn_id: entry.message_id.clone().unwrap_or_else(|| entry.id.clone()),
        timestamp: chrono::DateTime::parse_from_rfc3339(&entry.at)
            .ok()
            .map(|at| at.timestamp_millis()),
    }
}

fn record_returned_prompt(
    project_id: &str,
    session_id: &str,
    send: &LastChatSend,
    message_id: Option<String>,
) -> SessionChatReturnedPrompt {
    let entry = SessionChatReturnedPrompt {
        id: uuid::Uuid::new_v4().to_string(),
        text: send.text.clone(),
        image_paths: send.image_paths.clone(),
        retraction_pending: message_id.is_some(),
        message_id,
        at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        stored_at: Instant::now(),
    };
    if let Ok(mut entries) = store().lock() {
        entries.insert(store_key(project_id, session_id), entry.clone());
    }
    entry
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Screen text as the paste verifier compares it: no whitespace, no box or
/// rule drawing, no control characters.
fn normalize_screen_text(text: &str) -> String {
    text.chars()
        .filter(|character| {
            !character.is_whitespace()
                && !matches!(character, '\u{2500}'..='\u{259f}')
                && !character.is_control()
        })
        .collect::<String>()
        .to_lowercase()
}

/// Whether the composer holds the sent text: the normalized composer starts
/// with the head of the normalized message, or shows Claude's collapsed paste
/// placeholder for a body too large to paint.
fn composer_holds_sent_text(composer_text: &str, sent_text: &str) -> bool {
    let composer = normalize_screen_text(composer_text);
    if composer.is_empty() {
        return false;
    }
    if composer.starts_with(RETURNED_PROMPT_PASTED_PLACEHOLDER) {
        return true;
    }
    let sent = normalize_screen_text(sent_text);
    if sent.is_empty() {
        return false;
    }
    let head: String = sent.chars().take(RETURNED_PROMPT_MATCH_CHARS).collect();
    composer.starts_with(&head)
}

fn message_text(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the transcript says about the prompt: `Ok(Some(id))` when its user
/// row is the newest row, `Ok(None)` when Claude never wrote it, `Err(())`
/// when something followed it (the agent did take it).
fn transcript_row_for_send(
    messages: &[SessionChatMessage],
    send: &LastChatSend,
) -> Result<Option<String>, ()> {
    let Some(last) = messages.last() else {
        return Ok(None);
    };
    if last.role != SessionChatRole::User || last.source != SessionChatSource::Transcript {
        // The newest row is the agent's own output or a harness row: whatever
        // this prompt was, Claude answered it or never recorded it. A trailing
        // interrupt marker means the same thing.
        return if last.role == SessionChatRole::System {
            Err(())
        } else {
            Ok(None)
        };
    }
    if last.queued {
        return Err(());
    }
    let recorded = normalize_screen_text(&message_text(last));
    let sent = normalize_screen_text(&send.text);
    let matches_text = !sent.is_empty() && recorded.starts_with(&sent)
        || (!recorded.is_empty() && sent.starts_with(&recorded));
    let recent = last.timestamp.map_or(true, |timestamp| {
        timestamp >= send.sent_at_ms - RETURNED_PROMPT_ROW_SLACK_MS
    });
    if matches_text && recent {
        Ok(Some(last.id.clone()))
    } else {
        Ok(None)
    }
}

fn runtime_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn log(state: &AppState, level: LogLevel, event: &str, details: Value) {
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

/// Runs after the interrupt endpoint wrote Escape. Returns without doing
/// anything unless the session runs Claude Code and the Escape answers a send
/// this daemon made from Chat.
pub(crate) fn schedule_session_chat_returned_prompt_detection(
    state: &AppState,
    target: &SessionChatSendTarget,
    request_id: &str,
) {
    let terminal_agent = session_chat_composer_agent_id(&target.session)
        .or_else(|| session_chat_agent_for_session(&target.session));
    let Some(agent) = normalize_agent_id(terminal_agent.as_deref()) else {
        return;
    };
    if !matches!(agent.as_str(), "claude" | "openclaude") {
        return;
    }
    if resolve_session_chat_transcript_agent(Some(agent.as_str()))
        != Some(SessionChatTranscriptAgent::Claude)
    {
        return;
    }
    let Some(send) = take_last_chat_send(&target.project_id, &target.session_id) else {
        return;
    };
    if send.started_at.elapsed() > LAST_CHAT_SEND_MAX_AGE {
        return;
    }
    let state = state.clone();
    let project_id = target.project_id.clone();
    let session_id = target.session_id.clone();
    let zmx_name = target.zmx_name.clone();
    let session = target.session.clone();
    let request_id = request_id.to_string();
    tokio::spawn(async move {
        if !send.submitted {
            /*
            Escape landed while the send worker was still typing: the cancelled
            job never wrote Enter, so the message never reached Claude and the
            client keeps its own copy (`sendCancelled`). Only the half-pasted
            text sitting in the terminal composer is ours to remove.
            */
            enqueue_session_chat_send(
                &project_id,
                &session_id,
                &zmx_name,
                "session-chat-interrupt-clear",
                build_agent_tui_clear_input_steps(None, &send.text),
            );
            log(
                &state,
                LogLevel::Info,
                "sessionChatInterruptCancelledSendCleared",
                json!({ "projectId": project_id, "sessionId": session_id, "textBytes": send.text.len() }),
            );
            return;
        }
        tokio::time::sleep(RETURNED_PROMPT_DETECT_SETTLE).await;
        let started = Instant::now();
        let mut returned = false;
        loop {
            if let Some(capture) = capture_session_terminal_text(&zmx_name).await {
                // Not the readiness signature: that one admits a single input
                // row only, and a wrapped or multi-line prompt fills several.
                if let Some(composer_text) = claude_composer_input_text(&capture) {
                    if composer_holds_sent_text(&composer_text, &send.text) {
                        returned = true;
                        break;
                    }
                }
            }
            if started.elapsed() >= RETURNED_PROMPT_DETECT_TIMEOUT {
                break;
            }
            tokio::time::sleep(RETURNED_PROMPT_DETECT_POLL).await;
        }
        if !returned {
            return;
        }
        let transcript_path: Option<PathBuf> = {
            let agent_session_id = runtime_text(&session, "agentSessionId");
            let agent_session_path = runtime_text(&session, "agentSessionPath");
            tokio::task::spawn_blocking(move || {
                crate::session_chat_paths::resolve_session_chat_transcript_path(
                    SessionChatTranscriptAgent::Claude,
                    agent_session_id.as_deref(),
                    agent_session_path.as_deref(),
                )
            })
            .await
            .ok()
            .flatten()
        };
        let message_id = match transcript_path {
            None => None,
            Some(path) => {
                let page = tokio::task::spawn_blocking(move || {
                    crate::session_chat_tail::read_session_chat_tail_page(
                        SessionChatTranscriptAgent::Claude,
                        &path,
                        RETURNED_PROMPT_TAIL_LIMIT,
                        None,
                    )
                })
                .await;
                match page {
                    Ok(Ok(SessionChatTailPage::Page { messages, .. })) => {
                        match transcript_row_for_send(&messages, &send) {
                            Ok(message_id) => message_id,
                            Err(()) => {
                                log(
                                    &state,
                                    LogLevel::Info,
                                    "sessionChatReturnedPromptRejectedByTranscript",
                                    json!({ "projectId": project_id, "sessionId": session_id }),
                                );
                                return;
                            }
                        }
                    }
                    _ => None,
                }
            }
        };
        // The delivery watchdog would otherwise wake at its deadline to explain
        // a message that is deliberately back in the composer.
        crate::session_chat_watchdog::cancel_session_chat_send_watchdog(&project_id, &session_id);
        let entry = record_returned_prompt(&project_id, &session_id, &send, message_id.clone());
        // Claude fires no hook for this, so nothing else ends the working claim.
        let idle_state = state.clone();
        let (idle_project_id, idle_session_id, idle_request_id) =
            (project_id.clone(), session_id.clone(), request_id.clone());
        let _ = tokio::task::spawn_blocking(move || {
            let mut params = Map::new();
            params.insert("projectId".to_string(), json!(idle_project_id));
            params.insert("sessionId".to_string(), json!(idle_session_id));
            params.insert("activity".to_string(), json!("idle"));
            crate::server::dispatch_agent_http_blocking(
                &idle_state,
                "/api/updateAgentActivity".to_string(),
                idle_request_id,
                params,
            )
        })
        .await;
        // State frame: working=false plus `returnedPrompt`; the follower's next
        // tick retracts the row.
        crate::session_chat_options::session_chat_terminal_notice_publisher(
            &state,
            &project_id,
            &session_id,
        )();
        enqueue_session_chat_send(
            &project_id,
            &session_id,
            &zmx_name,
            "session-chat-returned-prompt-clear",
            build_agent_tui_clear_input_steps(None, &send.text),
        );
        log(
            &state,
            LogLevel::Info,
            "sessionChatReturnedPromptDetected",
            json!({
                "projectId": project_id,
                "sessionId": session_id,
                "returnedPromptId": entry.id,
                "messageId": message_id,
                "textBytes": send.text.len(),
            }),
        );
    });
}
