use super::*;

/// A chat send to a DeepSeek Harness session through Ghostex's dsh-tui plugin, else dsh-tui's
/// inject socket (dsh_chat_channel.rs): nothing is pasted into the terminal or read back from it,
/// and the TUI shows the message as if it had been typed. `None` when the screen path has to send
/// it (no plugin and no usable socket, or a slash command, an image or an interrupting send the
/// socket cannot take); each fallback is logged (CDXC:DshChatChannel 2026-11-05).
pub(super) async fn send_dsh_channel_message(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    text: &str,
    image_paths: &[String],
    interrupt: bool,
    source: SessionChatMessageSource,
    drafted_text: &str,
    draft_before_send: Option<&crate::session_chat_queue::SessionChatDraft>,
    draft_version: Option<&crate::session_chat_draft_versions::DraftVersion>,
) -> Option<std::result::Result<usize, DomainStateError>> {
    match crate::dsh_chat_channel::send(target, text, image_paths, interrupt).await? {
        Ok(_) => {}
        Err(crate::pi_chat_channel::ChannelError::Failed(message)) => {
            return Some(Err(DomainStateError {
                code: "invalidState",
                message: format!("DeepSeek Harness refused the message: {message}"),
            }));
        }
        // It may have gone through, so typing it as well could send it twice.
        Err(_) => {
            return Some(Err(DomainStateError {
                code: "deliveryUnconfirmed",
                message: "DeepSeek Harness took the message but did not confirm it. Check the chat before sending it again."
                    .to_string(),
            }));
        }
    }
    let (project_id, session_id) = (target.project_id.as_str(), target.session_id.as_str());
    crate::telemetry::prompt_sent(
        &target.session,
        match source {
            SessionChatMessageSource::Composer => "chat",
            _ => "queue",
        },
    );
    promote_draft_session_after_send(state, project_id, session_id);
    unpark_session_after_send(state, project_id, session_id);
    crate::session_chat_returned_prompt::record_session_chat_send_submitted(project_id, session_id);
    if let Some(version) = draft_version {
        if let Ok(db) = open_gxserver_database(&state.paths) {
            let _ =
                crate::session_chat_draft_versions::consume(&db, project_id, session_id, version);
        }
        broadcast_session_chat_queue_state(state, project_id, session_id);
    } else if source == SessionChatMessageSource::Composer {
        retire_sent_session_chat_draft(
            state,
            project_id,
            session_id,
            drafted_text,
            draft_before_send,
        );
    }
    Some(Ok(text.len()))
}
