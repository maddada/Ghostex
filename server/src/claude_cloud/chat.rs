//! A cloud session's conversation for its terminal client (`conversation`), and chat sends to a
//! cloud session: they go to claude.ai, never into its terminal.

use serde_json::{json, Map, Value};

use super::record::{session_cloud_record, CloudState};
use super::remote;
use super::start::{load, open, send_to_cloud_session, start_cloud_session};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

fn ids(params: &Map<String, Value>) -> (String, String) {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    (text("projectId"), text("sessionId"))
}

/// `conversation {projectId, sessionId}`: what claude.ai shows for the session (its state, what it
/// asks of the user, its branch) and its messages. A claude.ai that cannot be asked answers
/// `error`, which the terminal client shows instead of the conversation.
pub(crate) fn read_conversation(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (project_id, session_id) = ids(params);
    let (login, cloud_session_id) = {
        let db = open(state)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let (_, record) = load(&repository, &project_id, &session_id)?;
        let Some(cloud_session_id) = record.cloud_session_id() else {
            return Ok(json!({ "turns": [] }));
        };
        match remote::login_for(&state.paths.home_dir, &db, &record) {
            Ok(login) => (login, cloud_session_id),
            Err(error) => return Ok(json!({ "turns": [], "error": error })),
        }
    };
    let read = remote::read_session(&login, &cloud_session_id)
        .and_then(|session| Ok((session, remote::read_events(&login, &cloud_session_id)?)));
    let (session, events) = match read {
        Ok(read) => read,
        Err(error) => return Ok(json!({ "turns": [], "error": error })),
    };
    let turns: Vec<Value> = remote::conversation(&events)
        .into_iter()
        .map(|turn| json!({ "from": turn.from, "text": turn.text }))
        .collect();
    Ok(json!({
        "turns": turns,
        "status": session.status.map(remote::RemoteStatus::as_str),
        "needsAction": session.needs_action,
        "detail": session.detail,
        "failure": session.failure,
        "branch": session.branch,
        "updatedAt": session.updated_at,
    }))
}

/// A chat message (the composer, a queued prompt, `ghostex agents create --task` or `agents send`)
/// to a cloud session; `None` for every other session. A draft or failed start starts with it as
/// its task, a running session gets it as a follow-up.
///
/// CDXC:ClaudeCloud 2026-10-26 WHY: a cloud session's terminal runs Ghostex's cloud client, so a chat send typed there as a paste reached the client's one-line prompt: `ghostex agents create claude-cloud --task …` started the session with the paste's last line ("Reply to: …" and the paste's end code) as its task. The text goes to claude.ai whole instead.
pub(crate) fn send_chat_text(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    has_images: bool,
) -> Option<Result<(), DomainStateError>> {
    let record = {
        let db = open(state).ok()?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository.get_session(project_id, session_id).ok()??;
        session_cloud_record(&session)?
    };
    if has_images {
        return Some(Err(DomainStateError::bad_request(
            "Claude's cloud takes text only here. Send the text, or add the images on the session's claude.ai page.",
        )));
    }
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    Some(match record.state {
        CloudState::Draft | CloudState::Failed
            if !super::start::is_starting(project_id, session_id) =>
        {
            // The start takes seconds to minutes; the session's terminal shows it.
            params.insert("task".to_string(), json!(text));
            let state = state.clone();
            std::thread::spawn(move || {
                let _ = start_cloud_session(&state, &params);
            });
            Ok(())
        }
        CloudState::Draft | CloudState::Failed | CloudState::Starting => {
            Err(DomainStateError::bad_request(
                "This session is still starting in Claude's cloud. Send this once it runs.",
            ))
        }
        CloudState::Running => {
            params.insert("text".to_string(), json!(text));
            send_to_cloud_session(state, &params).map(|_| ())
        }
        CloudState::Moved => Err(DomainStateError::bad_request(
            "This session continues on this computer. Send this in its new row.",
        )),
    })
}
