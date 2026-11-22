//! The first chat message of a draft whose Run on row picked an agentbox box: it creates the box
//! instead of being typed into a terminal (agents/draft_run_location.rs).

use super::*;

/// Sends `text` as the first message of a pending box draft, or answers `None` when the session is
/// not one and the ordinary send path applies.
///
/// The send succeeds once the box launch is on its way: the draft is promoted (the chat hands the
/// thread to its terminal, where the box boots) and the message rides the launch as the agent's
/// first prompt, or, past the launch's inline limit, is typed once the box's agent shows its input.
pub(super) async fn send_pending_box_first_message(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    image_paths: &[String],
    composer_send: bool,
    draft_before_send: Option<&crate::session_chat_queue::SessionChatDraft>,
    draft_version: Option<&crate::session_chat_draft_versions::DraftVersion>,
) -> Option<std::result::Result<usize, DomainStateError>> {
    let pending = open_gxserver_database(&state.paths).ok().and_then(|db| {
        DomainRepository::new(&db, state.metadata.server_id.as_str())
            .get_session(project_id, session_id)
            .ok()
            .flatten()
            .and_then(|session| crate::agentbox::pending_session_agentbox(&session))
    });
    pending.as_ref()?;
    if !image_paths.is_empty() {
        return Some(Err(DomainStateError::bad_request(
            "The first message to a box cannot carry images. Send the text first, then attach images once the box is running.",
        )));
    }
    let blocking_state = state.clone();
    let (project, session, message) = (
        project_id.to_string(),
        session_id.to_string(),
        text.to_string(),
    );
    let launched = tokio::task::spawn_blocking(move || {
        let db =
            open_gxserver_database(&blocking_state.paths).map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("SQLite gxserver state error: {error}"),
            })?;
        let repository = DomainRepository::new(&db, blocking_state.metadata.server_id.as_str());
        let context = crate::zmx::ZmxServerContext {
            auth_token_file: blocking_state
                .paths
                .auth_token_file
                .to_string_lossy()
                .to_string(),
            base_url: format!(
                "http://{}:{}",
                blocking_state.config.listeners.local.host,
                blocking_state.config.listeners.local.port
            ),
        };
        let launched = crate::agents::launch_pending_box_draft(
            &repository,
            &db,
            &project,
            &session,
            &message,
            &context,
        )
        .map_err(|error| match error {
            crate::agents::AgentEndpointError::Domain(error) => error,
            crate::agents::AgentEndpointError::DependencyUnavailable(message) => DomainStateError {
                code: "dependencyUnavailable",
                message,
            },
        })?;
        let _ = schedule_presentation_session_delta(
            &blocking_state,
            &db,
            &repository,
            &project,
            &session,
        );
        Ok::<_, DomainStateError>(launched)
    })
    .await;
    let launched = match launched {
        Ok(Ok(Some(launched))) => launched,
        // Promoted between the read above and the launch: the ordinary path sends it.
        Ok(Ok(None)) => return None,
        Ok(Err(error)) => return Some(Err(error)),
        Err(error) => {
            return Some(Err(DomainStateError {
                code: "internalError",
                message: format!("Box launch task failed: {error}"),
            }))
        }
    };
    if !launched.prompt_in_launch {
        let (state, project_id, session_id, text) = (
            state.clone(),
            project_id.to_string(),
            session_id.to_string(),
            text.to_string(),
        );
        tokio::spawn(async move {
            if crate::agentbox::wait_for_box_agent_input(
                &state.paths,
                state.metadata.server_id.as_str(),
                &project_id,
                &session_id,
            )
            .await
            {
                crate::server::send_worktree_session_first_prompt(
                    &state,
                    &project_id,
                    &session_id,
                    &text,
                );
            }
        });
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
