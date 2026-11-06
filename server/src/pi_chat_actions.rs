//! Chat actions a Pi or OMP session takes through its chat channel (pi_chat_channel.rs, whose
//! CDXC:PiChatChannel note says why) instead of through its terminal: the Terminal and Chat draft
//! handoff through Pi's input box API, question answers through the dialogs the extension sees,
//! and the model and thinking level the chat's pills show. Each returns `None` when the screen path
//! has to do it (no channel, an older extension, nothing it can do), and logs that fallback.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::{
    domain::{DomainRepository, DomainStateError},
    pi_chat_channel::ChannelError,
    server::AppState,
    session_chat_send::SessionChatSendTarget,
    storage::open_gxserver_database,
};

fn internal(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: message.into(),
    }
}

/// True for a session whose terminal runs Pi or OMP.
pub(crate) fn is_pi_family(session: &Value) -> bool {
    matches!(
        crate::session_chat_composer::session_chat_composer_agent_id(session).as_deref(),
        Some("pi" | "omp")
    )
}

fn label(session: &Value) -> &'static str {
    match crate::session_chat_composer::session_chat_composer_agent_id(session).as_deref() {
        Some("omp") => "OMP",
        _ => "Pi",
    }
}

/// `kind` through the session's channel when it is open and has `capability`. `None` (logged as
/// a fallback for `action`) when the screen path is still safe: no channel, a command this
/// extension cannot run, or one nobody picked up.
async fn request(
    target: &SessionChatSendTarget,
    action: &str,
    capability: &str,
    kind: &'static str,
    params: Value,
    timeout: Duration,
) -> Option<Result<Value, ChannelError>> {
    let (project_id, session_id) = (target.project_id.clone(), target.session_id.clone());
    let reason = match crate::pi_chat_channel::live(&project_id, &session_id) {
        None => Some("no chat channel (older extension or agent not running)".to_string()),
        Some(channel) if !channel.supports(capability) => {
            Some(format!("extension has no {capability} command"))
        }
        Some(_) => None,
    };
    if let Some(reason) = reason {
        crate::pi_chat_channel::log_screen_fallback(&project_id, &session_id, action, &reason);
        return None;
    }
    let answer = tokio::task::spawn_blocking(move || {
        crate::pi_chat_channel::request(&project_id, &session_id, kind, params, timeout)
    })
    .await
    .unwrap_or(Err(ChannelError::NoAnswer));
    match answer {
        Err(reason @ (ChannelError::Unavailable(_) | ChannelError::Unsupported(_))) => {
            crate::pi_chat_channel::log_screen_fallback(
                &target.project_id,
                &target.session_id,
                action,
                &reason.to_string(),
            );
            None
        }
        other => Some(other),
    }
}

/// Terminal to Chat: the text in Pi's input box moves into the chat, read and cleared through
/// `ctx.ui.getEditorText` / `setEditorText` instead of the Ctrl+G prompt-editor handshake. The text
/// is saved to Saved Prompts before the box is cleared, and only that same text is cleared.
pub(crate) async fn editor_draft_to_chat(
    state: &AppState,
    target: &SessionChatSendTarget,
) -> Option<Result<Value, DomainStateError>> {
    let read = request(
        target,
        "draftToChat",
        "editor",
        "editor",
        json!({ "action": "read" }),
        Duration::from_secs(5),
    )
    .await?;
    Some(
        async {
            let text = read
                .map_err(|error| internal(error.to_string()))?
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if text.trim().is_empty() {
                return Ok(json!({ "content": "", "transferred": false }));
            }
            let saved = {
                let db = open_gxserver_database(&state.paths)
                    .map_err(|error| internal(error.to_string()))?;
                let mut params = Map::new();
                params.insert("content".to_string(), json!(text));
                params.insert("draftHandoff".to_string(), json!(true));
                params.insert("projectId".to_string(), json!(target.project_id));
                params.insert("sessionId".to_string(), json!(target.session_id));
                DomainRepository::new(&db, state.metadata.server_id.as_str())
                    .save_stashed_prompt(&params)?
            };
            let prompt_id = saved
                .pointer("/prompt/promptId")
                .and_then(Value::as_str)
                .ok_or_else(|| internal("The terminal draft could not be saved."))?
                .to_string();
            // A box the user typed into since the read keeps its text; the saved copy is in chat.
            let cleared = request(
                target,
                "draftToChat",
                "editor",
                "editor",
                json!({ "action": "clear", "expected": text }),
                Duration::from_secs(5),
            )
            .await;
            if !matches!(&cleared, Some(Ok(result)) if result["cleared"] == true) {
                crate::pi_chat_channel::log_screen_fallback(
                    &target.project_id,
                    &target.session_id,
                    "draftToChat",
                    "the input box changed after it was read, so it was left as it is",
                );
            }
            crate::session_chat_send::read_and_release_stashed_prompt(
                state,
                &target.project_id,
                &target.session_id,
                &prompt_id,
                saved
                    .get("created")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                saved
                    .get("draftVersion")
                    .filter(|version| !version.is_null())
                    .cloned()
                    .and_then(|version| serde_json::from_value(version).ok()),
            )
        }
        .await,
    )
}

/// Chat to Terminal: the chat's draft is put in Pi's input box with `setEditorText`, not typed. Text
/// the box held before is kept in Saved Prompts, as the prompt-editor route keeps it.
pub(crate) async fn place_editor_draft(
    state: &AppState,
    target: &SessionChatSendTarget,
    content: &str,
) -> Option<Result<Value, DomainStateError>> {
    let replaced = request(
        target,
        "draftToTerminal",
        "editor",
        "editor",
        json!({ "action": "replace", "text": content }),
        Duration::from_secs(5),
    )
    .await?;
    Some((|| {
        let result = replaced.map_err(|error| DomainStateError {
            code: "sessionInputFailed",
            message: error.to_string(),
        })?;
        let previous = result["previous"].as_str().unwrap_or_default();
        if !previous.trim().is_empty() && previous != content {
            let db = open_gxserver_database(&state.paths)
                .map_err(|error| internal(error.to_string()))?;
            let mut params = Map::new();
            params.insert("content".to_string(), json!(previous));
            params.insert("projectId".to_string(), json!(target.project_id));
            params.insert("sessionId".to_string(), json!(target.session_id));
            DomainRepository::new(&db, state.metadata.server_id.as_str())
                .save_stashed_prompt(&params)?;
        }
        Ok(json!({ "replaced": true }))
    })())
}

/// A question card's answer, given to the dialogs the question opened in Pi (the extension sees
/// every `ctx.ui.select` / `input` / `confirm`, chat_channel_source.rs `watchDialogs`), so no
/// arrow keys are aligned against the screen. `None` sends it to the key plan: no channel, no
/// dialog open, or a multi-choice answer (OMP's multi-select dialog keeps its own ticks).
pub(crate) async fn answer_questions(
    target: &SessionChatSendTarget,
    questions: &[crate::session_chat::SessionChatQuestion],
    selections: &[crate::session_chat::SessionChatQuestionSelection],
) -> Option<Result<(), DomainStateError>> {
    let mut answers = Vec::new();
    for (index, question) in questions.iter().enumerate() {
        let selection = selections.get(index);
        let other = selection
            .and_then(|selection| selection.other.as_deref())
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let picked = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        if picked.len() > 1 || (question.multi_select && !picked.is_empty()) {
            crate::pi_chat_channel::log_screen_fallback(
                &target.project_id,
                &target.session_id,
                "answerQuestion",
                "a multi-choice answer",
            );
            return None;
        }
        let option = picked
            .first()
            .and_then(|index| question.options.get(*index))
            .map(|option| option.label.clone());
        match (option, other) {
            (Some(label), _) => answers.push(json!({ "label": label })),
            (None, Some(text)) => answers.push(json!({ "text": text })),
            // An unanswered question ends the agent's question loop, as Esc does.
            (None, None) => {
                answers.push(json!({ "cancel": true }));
                break;
            }
        }
    }
    let answered = request(
        target,
        "answerQuestion",
        "dialogs",
        "answerDialogs",
        json!({ "answers": answers }),
        Duration::from_secs(10 + 6 * answers.len() as u64),
    )
    .await?;
    Some(answered.map(|_| ()).map_err(|error| DomainStateError {
        code: crate::session_chat_send::ANSWER_IN_TERMINAL,
        message: format!(
            "{} did not take the answer: {error}. Answer it in the terminal.",
            label(&target.session)
        ),
    }))
}

/// Pi's own state for the chat's option detection: the model (`provider/id` and its name) and
/// thinking level it runs now, instead of its footer. `None` without an open channel, and while a
/// dialog is open, whose notice still comes from the screen.
pub(crate) fn channel_state(project_id: &str, session_id: &str) -> Option<Value> {
    let channel = crate::pi_chat_channel::live(project_id, session_id)?;
    if !channel.supports("state") {
        return None;
    }
    let state = crate::pi_chat_channel::request(
        project_id,
        session_id,
        "state",
        json!({}),
        Duration::from_secs(3),
    )
    .ok()?;
    state["dialogs"]
        .as_array()
        .is_none_or(|dialogs| dialogs.is_empty())
        .then_some(state)
}

/// Whether a new session's first prompt (a worktree's, an automation's) goes through the chat send
/// instead of being typed after a screen wait: OpenCode takes it through its service, and Pi and OMP
/// through their chat channel once it opens. The chat queue holds it as a startup send until the
/// agent can take it, the same way a new chat's first message waits.
pub(crate) fn first_prompt_through_chat(session: &Value) -> bool {
    is_pi_family(session)
        || crate::session_chat_follower::session_chat_agent_for_session(session).as_deref()
            == Some("opencode")
}

/// Queues `text` as the session's startup send (see `first_prompt_through_chat`).
pub(crate) fn queue_first_prompt(
    paths: &crate::paths::GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("text".to_string(), json!(text));
    params.insert("startupSend".to_string(), json!(true));
    crate::session_chat_queue::handle_session_chat_queue_endpoint(
        paths,
        server_id,
        "/api/queueSessionChatPrompt",
        &params,
    )
    .map(|_| ())
}
