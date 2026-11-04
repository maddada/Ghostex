use super::*;

/// A chat send to a Claude cloud session: the text goes to claude.ai (claude_cloud/chat.rs)
/// instead of being typed into the session's terminal, which runs Ghostex's cloud client. `None`
/// for every other session.
pub(super) async fn send_claude_cloud_message(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
    composer_send: bool,
    draft_before_send: Option<&crate::session_chat_queue::SessionChatDraft>,
    draft_version: Option<&crate::session_chat_draft_versions::DraftVersion>,
) -> Option<std::result::Result<usize, DomainStateError>> {
    let blocking_state = state.clone();
    let (project, session, message) = (
        project_id.to_string(),
        session_id.to_string(),
        text.to_string(),
    );
    let has_images = !image_paths.is_empty();
    let sent = tokio::task::spawn_blocking(move || {
        crate::claude_cloud::send_chat_text(
            &blocking_state,
            &project,
            &session,
            &message,
            has_images,
        )
    })
    .await;
    match sent {
        Ok(None) => return None,
        Ok(Some(Err(error))) => return Some(Err(error)),
        Ok(Some(Ok(()))) => {}
        Err(error) => {
            return Some(Err(DomainStateError {
                code: "internalError",
                message: format!("Claude cloud send task failed: {error}"),
            }))
        }
    }
    unpark_session_after_send(state, project_id, session_id);
    crate::session_chat_returned_prompt::record_session_chat_send_submitted(project_id, session_id);
    if let Some(version) = draft_version {
        if let Ok(db) = open_gxserver_database(&state.paths) {
            let _ =
                crate::session_chat_draft_versions::consume(&db, project_id, session_id, version);
        }
        broadcast_session_chat_queue_state(state, project_id, session_id);
    } else if composer_send {
        retire_sent_session_chat_draft(state, project_id, session_id, text, draft_before_send);
    }
    Some(Ok(text.len()))
}
